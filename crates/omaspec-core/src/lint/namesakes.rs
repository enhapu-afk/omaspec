//! A state name written again in another composite state or region. In
//! PlantUML a name is one state in the whole diagram, so the second `X`
//! is the first one, and a transition to it leaves the state it is written
//! in. Mostly that is a namesake meant as a state of its own, and the
//! diagram draws one box where the writer thought of two.

use std::collections::{HashMap, HashSet};

use super::{Finding, Row, Severity};
use crate::text::re;

/// Where a line stands: the composite states around it, outermost first,
/// each with its region (0 before any `--` or `||`).
type Place = Vec<(String, usize)>;

/// A warning where a state's name stands in a composite state or region
/// that neither holds nor lies in the one it was first written in.
pub(super) fn namesakes(rows: &[Row], findings: &mut Vec<Finding>) {
    let mut place: Place = Vec::new();
    // Each name's first place and its line.
    let mut home: HashMap<String, (Place, usize)> = HashMap::new();
    let mut told: HashSet<(String, Place)> = HashSet::new();
    for r in rows {
        let text = r.text.trim();
        if text == "}" {
            place.pop();
            continue;
        }
        if text.len() >= 2 && (text.chars().all(|c| c == '-') || text.chars().all(|c| c == '|')) {
            if let Some((_, region)) = place.last_mut() {
                *region += 1;
            }
            continue;
        }
        let mut names = Vec::new();
        let mut opens = None;
        if let Some(c) = re!(r#"(?i)^state\s+(?:"[^"]*"\s+as\s+)?([\w.]+)"#).captures(text) {
            names.push(c[1].to_owned());
            if text.ends_with('{') {
                opens = Some(c[1].to_owned());
            }
        } else if let Some(c) =
            re!(r"^([\w.]+|\[\*\])\s*-[-\w\[\],#]*>\s*([\w.]+|\[\*\])").captures(text)
        {
            names.push(c[1].to_owned());
            names.push(c[2].to_owned());
        }
        // `P.X` says which one it means.
        for name in names.into_iter().filter(|n| n != "[*]" && !n.contains('.')) {
            let Some((first, line)) = home.get(&name) else {
                home.insert(name, (place.clone(), r.row));
                continue;
            };
            if place.starts_with(first) || first.starts_with(&place) {
                continue;
            }
            if !told.insert((name.clone(), place.clone())) {
                continue;
            }
            findings.push(Finding::new(
                r.row,
                Severity::Warning,
                format!(
                    "{name} here is the state {name} {} (line {}): a name is one state in the whole diagram. If this is another state, give it a name of its own",
                    where_(first, &place),
                    line + 1
                ),
            ));
        }
        if let Some(name) = opens {
            place.push((name, 0));
        }
    }
}

/// Where the place `first` is, seen from `here`: `in P`, or `in another
/// region of P` when both are in one state.
fn where_(first: &Place, here: &Place) -> String {
    let shared = first.iter().zip(here).take_while(|(a, b)| a == b).count();
    match (first.get(shared), here.get(shared)) {
        (Some((state, _)), Some((other, _))) if state == other && first.len() == shared + 1 => {
            format!("in another region of {state}")
        }
        _ => match first.last() {
            Some((state, _)) => format!("in {state}"),
            None => "outside any state".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::lang::Language;
    use crate::lint::lint;

    fn warned(body: &str) -> Vec<(usize, String)> {
        let code = format!("@startuml\n{body}@enduml\n");
        lint(&code, Language::PlantUml)
            .into_iter()
            .filter(|f| f.message.contains("is one state in the whole diagram"))
            .map(|f| (f.line + 1, f.message))
            .collect()
    }

    #[test]
    fn a_name_in_two_composite_states_is_one_state() {
        let found = warned(
            "[*] --> A\nA --> P\nA --> Q\nstate P {\n  [*] --> X\n  X --> [*]\n}\nstate Q {\n  [*] --> X\n  X --> [*]\n}\n",
        );
        assert_eq!(
            found,
            [(
                10,
                "X here is the state X in P (line 6): a name is one state in the whole diagram. If this is another state, give it a name of its own".to_owned()
            )]
        );
    }

    #[test]
    fn regions_of_one_state_are_apart_too() {
        let found = warned(
            "state S {\n  [*] --> Ask\n  Ask --> Wait\n  ||\n  [*] --> Watch\n  Watch --> Ask : again\n}\n",
        );
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].0, 7);
        assert!(
            found[0].1.contains("in another region of S (line 3)"),
            "{}",
            found[0].1
        );
    }

    #[test]
    fn a_state_reached_from_around_it_or_within_it_is_no_namesake() {
        // From outside into a nested state, from a nested state out, and a
        // dotted name that says which one it means.
        let body = "[*] --> P\nstate P {\n  [*] --> X\n  state R {\n    [*] --> Y\n    Y --> X : up\n  }\n}\nX --> Done\nstate Q {\n  [*] --> P.X\n}\n";
        assert!(warned(body).is_empty(), "{:?}", warned(body));
    }
}
