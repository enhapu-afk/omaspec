//! Open Iconic names the set does not have (`<&camera>`). PlantUML draws
//! nothing in their place, and so does Omaspec: the icon is gone without a
//! word in the picture, so lint says it, with the names of the set that
//! come close.

use super::{Finding, Fix, Severity, distance};

/// How many names of the set a finding offers at most.
const OFFERED: usize = 3;

/// A warning for each `<&name>` of a PlantUML diagram whose name Open
/// Iconic does not have, and each C4 sprite `"&name"`, which is drawn as
/// one; comments are left alone, a block `/' … '/` too.
pub(super) fn findings(lines: &[&str], findings: &mut Vec<Finding>) {
    let mut in_block = false;
    for (row, line) in lines.iter().enumerate() {
        let text = line.trim();
        if in_block {
            in_block = !text.ends_with("'/");
            continue;
        }
        if let Some(rest) = text.strip_prefix("/'") {
            in_block = !rest.ends_with("'/");
            continue;
        }
        if text.starts_with('\'') {
            continue;
        }
        // Each name with the byte it starts at in the line.
        let tags = omaspec_uml::unknown_open_iconic(line)
            .into_iter()
            .map(|(tag, name)| {
                let at = tag.start + line[tag].find('&').map_or(0, |amp| amp + 1);
                (format!("<&{name}>"), name, at)
            });
        let sprites = sprites(line).map(|(name, at)| (format!("\"&{name}\""), name, at));
        for (written, name, at) in tags.chain(sprites) {
            let mut message = format!(
                "`{written}`: Open Iconic has no icon of that name, so nothing is drawn in its place"
            );
            let near = near(name);
            if !near.is_empty() {
                let names: Vec<String> = near.iter().map(|n| format!("`<&{n}>`")).collect();
                message.push_str(&format!("; it has {}", names.join(", ")));
            }
            // One name near it is the one meant.
            let fix = match near.as_slice() {
                [meant] => Some(Fix::replace(
                    format!("Write {meant}"),
                    row,
                    format!("{}{meant}{}", &line[..at], &line[at + name.len()..]),
                )),
                _ => None,
            };
            findings.push(Finding::new("unknown-icon", row, Severity::Warning, message).with(fix));
        }
    }
}

/// The Open Iconic names that the quoted words of `line` give a C4 sprite
/// (`$sprite="&camera"`) and the set does not have, each with the byte it
/// starts at.
fn sprites(line: &str) -> impl Iterator<Item = (&str, usize)> {
    line.match_indices("\"&").filter_map(move |(at, _)| {
        let rest = &line[at + 2..];
        let name = &rest[..rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(rest.len())];
        (!name.is_empty() && !omaspec_uml::unknown_open_iconic(&format!("<&{name}>")).is_empty())
            .then_some((name, at + 2))
    })
}

/// The names of the set `name` may have meant: those that go on from it
/// (`camera` → `camera-slr`), or else those an edit or two away.
fn near(name: &str) -> Vec<&'static str> {
    let longer: Vec<&str> = omaspec_uml::open_iconic_names()
        .filter(|n| {
            n.strip_prefix(name)
                .is_some_and(|rest| rest.starts_with('-'))
        })
        .take(OFFERED)
        .collect();
    if !longer.is_empty() {
        return longer;
    }
    let wanted: Vec<char> = name.chars().collect();
    let max = if wanted.len() < 5 { 1 } else { 2 };
    let mut close: Vec<(usize, &str)> = omaspec_uml::open_iconic_names()
        .filter_map(|n| {
            let found = distance(&wanted, &n.chars().collect::<Vec<_>>(), max);
            (found <= max).then_some((found, n))
        })
        .collect();
    close.sort_by_key(|&(found, _)| found);
    close.into_iter().take(OFFERED).map(|(_, n)| n).collect()
}

#[cfg(test)]
mod tests {
    use crate::lang::Language;
    use crate::lint::lint;

    #[test]
    fn a_name_open_iconic_lacks_is_named_with_the_names_it_has() {
        let found = lint(
            "@startuml\n' <&nothing> in a comment\nAlice -> Bob : <&camera> photo <&hart>\n@enduml\n",
            Language::PlantUml,
        );
        let messages: Vec<&str> = found
            .iter()
            .filter(|f| f.message.contains("Open Iconic"))
            .map(|f| f.message.as_str())
            .collect();
        assert_eq!(messages.len(), 2, "{found:?}");
        assert!(
            found
                .iter()
                .all(|f| f.line == 2 || !f.message.contains("Open Iconic"))
        );
        assert!(messages[0].contains("`<&camera-slr>`"), "{}", messages[0]);
        assert!(messages[1].contains("`<&heart>`"), "{}", messages[1]);
        // A C4 sprite is one too; a block comment is left alone.
        let found = lint(
            "@startuml\n!include <C4/C4_Container>\n/'\nPerson(a, \"A\", $sprite=\"&nothing\")\n'/\n/' <&nothing> '/\nPerson(u, \"User\", $sprite=\"&camera\")\nContainer(api, \"API\", \"Rust\", $sprite=\"&heart\")\n@enduml\n",
            Language::PlantUml,
        );
        let named: Vec<(usize, &str)> = found
            .iter()
            .filter(|f| f.message.contains("Open Iconic"))
            .map(|f| (f.line, f.message.as_str()))
            .collect();
        assert_eq!(named.len(), 1, "{found:?}");
        assert_eq!(named[0].0, 6);
        assert!(named[0].1.starts_with("`\"&camera\"`"), "{}", named[0].1);
        // A name the set has is no finding.
        assert!(
            lint(
                "@startuml\nAlice -> Bob : <&heart>\n@enduml\n",
                Language::PlantUml
            )
            .iter()
            .all(|f| !f.message.contains("Open Iconic"))
        );
    }
}
