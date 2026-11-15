//! Open Iconic names the set does not have (`<&camera>`). PlantUML draws
//! nothing in their place, and so does Omaspec: the icon is gone without a
//! word in the picture, so lint says it, with the names of the set that
//! come close.

use super::{Finding, Severity, distance};

/// How many names of the set a finding offers at most.
const OFFERED: usize = 3;

/// A warning for each `<&name>` of a PlantUML diagram whose name Open
/// Iconic does not have; comments are left alone.
pub(super) fn findings(lines: &[&str], findings: &mut Vec<Finding>) {
    for (row, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('\'') {
            continue;
        }
        for (_, name) in omaspec_uml::unknown_open_iconic(line) {
            let mut message = format!(
                "`<&{name}>`: Open Iconic has no icon of that name, so nothing is drawn in its place"
            );
            let near = near(name);
            if !near.is_empty() {
                let names: Vec<String> = near.iter().map(|n| format!("`<&{n}>`")).collect();
                message.push_str(&format!("; it has {}", names.join(", ")));
            }
            findings.push(Finding::new(row, Severity::Warning, message));
        }
    }
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
