//! Archimate-PlantUML (`!include <archimate/Archimate>`), read into the
//! structure engine as C4-PlantUML is ([`super::c4`]): the library is not
//! read, its macros are known by name and each is written as the PlantUML
//! it stands for, on the line it was written on.
//!
//! An element (`Business_Actor(customer, "Customer")`) is an `archimate`
//! box in the color of its layer with the icon of its type over its name;
//! a grouping a dashed frame; a relationship (`Rel_Serving(a, b, "x")`,
//! `Rel_Realization_Up(…)`) the line ArchiMate draws for it, as near as
//! PlantUML's arrows come: a serving or a triggering line with a head, an
//! access dotted, a realization dotted with a hollow triangle, a
//! specialization solid with one, an aggregation and a composition with
//! their diamonds. Its direction runs along the flow or within a rank as
//! a written arrow's does. Every other line is read as written.

use crate::Declined;
use crate::c4::{parse_args, split_args};
use crate::source::Line;

use super::Diagram;

/// The layers of ArchiMate, as its macros start.
const LAYERS: [&str; 8] = [
    "Business",
    "Application",
    "Technology",
    "Physical",
    "Implementation",
    "Motivation",
    "Strategy",
    "Other",
];

/// Whether a body includes Archimate-PlantUML: the include lines are
/// taken out of `raw`, which is then read by [`parse`].
pub(crate) fn take(raw: &mut Vec<Line<'_>>) -> bool {
    let before = raw.len();
    raw.retain(|line| !library(line.text));
    raw.len() != before
}

/// Whether an include line names the library: `<archimate/Archimate>`, its
/// address, or a copy (`Archimate.puml`).
fn library(text: &str) -> bool {
    let Some(rest) = text
        .strip_prefix("!includeurl")
        .or_else(|| text.strip_prefix("!include"))
    else {
        return false;
    };
    if !rest.starts_with(char::is_whitespace) {
        return false;
    }
    let target = rest.trim().trim_start_matches('<').trim_end_matches('>');
    let file = target.rsplit('/').next().unwrap_or(target);
    file.trim_end_matches(".puml")
        .eq_ignore_ascii_case("archimate")
}

/// The diagram `lines` describe, its macros written out.
pub(crate) fn parse(lines: &[Line<'_>]) -> Result<Diagram, Declined> {
    let mut out: Vec<(Line<'_>, String)> = Vec::with_capacity(lines.len());
    for line in lines {
        let text = match macro_of(line.text) {
            Some((name, args, rest)) => written(name, &args, rest, line.no)?,
            None => None,
        };
        out.push((*line, text.unwrap_or_else(|| line.text.to_owned())));
    }
    let written: Vec<Line<'_>> = out
        .iter()
        .map(|(model, text)| Line {
            text: text.as_str(),
            ..*model
        })
        .collect();
    super::parse(&written)
}

/// `Name(args) rest` as its name, its arguments and what follows.
fn macro_of(text: &str) -> Option<(&str, Vec<String>, &str)> {
    let open = text.find('(')?;
    let name = text[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let close = text.rfind(')')?;
    (close > open).then(|| {
        (
            name,
            split_args(&text[open + 1..close]),
            text[close + 1..].trim(),
        )
    })
}

/// The PlantUML a macro of the library stands for; `None` for a line that
/// is no macro of it.
fn written(name: &str, args: &[String], rest: &str, no: usize) -> Result<Option<String>, Declined> {
    let args = parse_args(args);
    let at = |i: usize| args.positional.get(i).map(|s| s.trim().to_owned());
    let alias = || {
        at(0)
            .filter(|a| !a.is_empty() && a.chars().all(|c| c.is_alphanumeric() || c == '_'))
            .ok_or_else(|| Declined::at(no, "an element needs a name of letters, digits and `_`"))
    };
    let label = |alias: &str| {
        let label = at(1)
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| alias.to_owned());
        if label.contains('"') {
            Err(Declined::at(
                no,
                "a quote inside a label is not supported yet",
            ))
        } else {
            Ok(label)
        }
    };
    if matches!(name, "Grouping" | "Group") {
        let alias = alias()?;
        let label = label(&alias)?;
        return Ok(Some(
            format!("rectangle \"{label}\" as {alias} #transparent;line.dashed {rest}")
                .trim_end()
                .to_owned(),
        ));
    }
    if let Some(kind) = name.strip_prefix("Rel_") {
        return relation(kind, &args.positional, no).map(Some);
    }
    if name.starts_with("Junction") {
        return Err(Declined::at(no, "ArchiMate's junctions are not drawn yet"));
    }
    let Some((layer, kind)) = name.split_once('_') else {
        return Ok(None);
    };
    if !LAYERS.contains(&layer) || kind.is_empty() {
        return Ok(None);
    }
    let alias = alias()?;
    let label = label(&alias)?;
    let icon = format!("{}-{}", layer.to_ascii_lowercase(), kebab(kind));
    if crate::icons::archimate::icon(&icon).is_none() {
        return Err(Declined::at(
            no,
            format!("ArchiMate's `{name}` is not drawn yet"),
        ));
    }
    let color = match layer {
        "Other" => "#FBB875".to_owned(),
        layer => format!("#{layer}"),
    };
    Ok(Some(
        format!("archimate {color} \"{label}\" as {alias} <<{icon}>> {rest}")
            .trim_end()
            .to_owned(),
    ))
}

/// A relationship: `Serving`, `Realization_Up`, `Access_rw`.
fn relation(kind: &str, args: &[String], no: usize) -> Result<String, Declined> {
    let (from, to) = match (args.first(), args.get(1)) {
        (Some(from), Some(to)) if !from.trim().is_empty() && !to.trim().is_empty() => {
            (from.trim(), to.trim())
        }
        _ => return Err(Declined::at(no, "a relationship needs two elements")),
    };
    let label = args.get(2).map(|l| l.trim()).unwrap_or_default();
    let (kind, direction) = ["Up", "Down", "Left", "Right"]
        .iter()
        .find_map(|dir| {
            kind.strip_suffix(dir)
                .and_then(|k| k.strip_suffix('_'))
                .map(|k| (k, Some(dir.to_ascii_lowercase())))
        })
        .unwrap_or((kind, None));
    // The line's tail, body and head, as PlantUML writes an arrow.
    let (tail, body, head) = match kind {
        "Access" | "Access_w" => ("", '.', ">"),
        "Access_r" => ("<", '.', ""),
        "Access_rw" => ("<", '.', ">"),
        "Aggregation" => ("o", '-', ""),
        "Composition" => ("*", '-', ""),
        "Association" => ("", '-', ""),
        "Association_dir" => ("", '-', ">"),
        "Flow" | "Influence" => ("", '.', ">"),
        "Realization" | "Realisation" => ("", '.', "|>"),
        "Specialization" | "Specialisation" => ("", '-', "|>"),
        "Assignment" | "Serving" | "Triggering" | "UsedBy" => ("", '-', ">"),
        other => {
            return Err(Declined::at(
                no,
                format!("ArchiMate's `Rel_{other}` is not drawn yet"),
            ));
        }
    };
    let arrow = match direction {
        Some(dir) => format!("{tail}{body}{dir}{body}{head}"),
        None => format!("{tail}{body}{body}{head}"),
    };
    if label.contains('"') {
        return Err(Declined::at(
            no,
            "a quote inside a label is not supported yet",
        ));
    }
    Ok(if label.is_empty() {
        format!("{from} {arrow} {to}")
    } else {
        format!("{from} {arrow} {to} : {label}")
    })
}

/// `SystemSoftware` as `system-software`.
fn kebab(kind: &str) -> String {
    let mut out = String::with_capacity(kind.len() + 4);
    for (i, c) in kind.chars().enumerate() {
        if c == '_' {
            out.push('-');
        } else if c.is_ascii_uppercase() {
            if i > 0 && !out.ends_with('-') {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written_line(text: &str) -> Result<Option<String>, Declined> {
        let (name, args, rest) = macro_of(text).expect("a macro");
        written(name, &args, rest, 3)
    }

    #[test]
    fn each_macro_is_written_as_the_plantuml_it_stands_for() {
        assert_eq!(
            written_line("Business_Actor(customer, \"Customer\")").unwrap(),
            Some("archimate #Business \"Customer\" as customer <<business-actor>>".into())
        );
        assert_eq!(
            written_line("Technology_SystemSoftware(os, \"Linux\")").unwrap(),
            Some("archimate #Technology \"Linux\" as os <<technology-system-software>>".into())
        );
        assert_eq!(
            written_line("Grouping(g, \"Front office\") {").unwrap(),
            Some("rectangle \"Front office\" as g #transparent;line.dashed {".into())
        );
        for (text, line) in [
            ("Rel_Serving(a, b, \"serves\")", "a --> b : serves"),
            ("Rel_Realization_Up(a, b)", "a .up.|> b"),
            ("Rel_Access_rw(a, b, \"r/w\")", "a <..> b : r/w"),
            ("Rel_Aggregation_Left(a, b)", "a o-left- b"),
            ("Rel_Composition(a, b)", "a *-- b"),
            ("Rel_Specialization(a, b)", "a --|> b"),
        ] {
            assert_eq!(written_line(text).unwrap(), Some(line.into()), "{text}");
        }
        // Not a macro of the library: as written.
        assert_eq!(written_line("Something(a, b)").unwrap(), None);
        for (text, part) in [
            ("Business_Unicorn(u, \"U\")", "not drawn"),
            ("Rel_Teleport(a, b)", "Rel_Teleport"),
            ("Junction_And(j, \"\")", "junctions"),
            ("Business_Actor(\"a b\", \"A\")", "name"),
        ] {
            let err = written_line(text).unwrap_err();
            assert_eq!(err.line, Some(3));
            assert!(err.reason.contains(part), "{text}: {}", err.reason);
        }
    }

    #[test]
    fn the_library_is_known_by_its_include() {
        assert!(library("!include <archimate/Archimate>"));
        assert!(library(
            "!includeurl https://raw.githubusercontent.com/plantuml-stdlib/Archimate-PlantUML/master/Archimate.puml"
        ));
        assert!(!library("!include <C4/C4_Container>"));
        // Drawn, through the structure engine.
        let source = "@startuml\n!include <archimate/Archimate>\nBusiness_Actor(customer, \"Customer\")\nBusiness_Service(claims, \"Claims\")\nRel_Serving(claims, customer, \"serves\")\n@enduml\n";
        let svg = crate::render(source).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(svg.contains("Customer") && svg.contains("#FFFFCC"), "{svg}");
    }
}
