//! Where a diagram's names are written, when the engine does not say.
//! The rules about names read [`Mention`]s, which the engine's own
//! readers keep as they read. A diagram the engine declines (a line it
//! does not draw yet) has none, so its lines are read here by pattern:
//! less exactly, and only then.

use omaspec_uml::Mention;
use regex::Regex;

use super::Row;
use crate::text::{re, unquote};

/// The participants of a PlantUML sequence diagram.
pub(super) fn plantuml_sequence(rows: &[Row]) -> Vec<Mention> {
    let mut names = Vec::new();
    for r in rows {
        if let Some(c) = re!(r"(?i)^(?:create\s+)?(?:actor|participant|boundary|control|entity|database|collections|queue)\s+(.+)$")
            .captures(r.text)
        {
            let Some(d) = re!(r#"(?i)^(?:"([^"]+)"|([^\s"<#]+))(?:\s+as\s+(?:"([^"]+)"|([^\s"<#]+)))?"#)
                .captures(&c[1])
            else {
                continue;
            };
            let g = |i: usize| d.get(i).map_or("", |m| m.as_str());
            let (key, shown) = if d.get(1).is_some() {
                let alias = if d.get(3).is_some() { g(3) } else { g(4) };
                (if alias.is_empty() { g(1) } else { alias }, g(1))
            } else if d.get(3).is_some() {
                (g(2), g(3))
            } else if d.get(4).is_some() {
                (g(4), g(2))
            } else {
                (g(2), "")
            };
            names.push(Mention::declared(key, shown, r.row));
        } else if let Some(c) = re!(r#"(?i)^(?:activate|deactivate|destroy)\s+("[^"]+"|\S+)"#).captures(r.text) {
            names.push(Mention::used(unquote(&c[1]), r.row));
        } else if let Some(c) = re!(r"(?i)^[hr]?note\s+(?:over|left of|right of)\s+([^:]+)")
            .captures(r.text)
            .or_else(|| re!(r"(?i)^ref\s+over\s+([^:]+)").captures(r.text))
        {
            names.extend(c[1].split(',').map(|k| Mention::used(unquote(k), r.row)));
        } else if let Some(c) = re!(r#"^("[^"]+"|[\p{L}\p{N}_.@]+)\s*([ox]?<{0,2}-+(?:\[[^\]]*\])?-*(?:>{1,2}|\\{1,2}|/{1,2})?[ox]?)\s*("[^"]+"|[\p{L}\p{N}_.@]+)\s*(?:\+\+|--|\*\*|!!)?\s*(?::\s*(.*))?$"#)
            .captures(r.text)
        {
            names.push(Mention::used(unquote(&c[1]), r.row));
            names.push(Mention::used(unquote(&c[3]), r.row));
        }
    }
    names
}

/// The participants of a Mermaid sequence diagram.
pub(super) fn mermaid_sequence(rows: &[Row]) -> Vec<Mention> {
    let mut names = Vec::new();
    for r in rows {
        if let Some(c) = re!(r"(?i)^(?:create\s+)?(?:participant|actor)\s+(.+?)(?:\s+as\s+(.+))?$")
            .captures(r.text)
        {
            let key = re!(r"@\{.*\}\s*$").replace(&c[1], "");
            let shown = c.get(2).map_or("", |m| m.as_str().trim());
            names.push(Mention::declared(key.trim(), shown, r.row));
        } else if let Some(c) =
            re!(r"(?i)^(?:activate|deactivate|destroy)\s+(.+)$").captures(r.text)
        {
            names.push(Mention::used(c[1].trim(), r.row));
        } else if let Some(c) =
            re!(r"(?i)^note\s+(?:over|left of|right of)\s+([^:]+):").captures(r.text)
        {
            names.extend(c[1].split(',').map(|k| Mention::used(k.trim(), r.row)));
        } else if let Some(c) = re!(
            r"^([^:;,<>+\s][^:;,<>+]*?)\s*(<<)?--?(?:>>|>|x|\))\s*[+-]?\s*([^:;,<>+]+?)\s*:\s*(.*)$"
        )
        .captures(r.text)
        {
            names.push(Mention::used(c[1].trim(), r.row));
            names.push(Mention::used(c[3].trim(), r.row));
        }
    }
    names
}

/// The states of a state diagram, each with the composite states around
/// its line: `declaration` and `transition` are the two lines of its
/// language that name one.
pub(super) fn states(rows: &[Row], transition: &Regex, declaration: &Regex) -> Vec<Mention> {
    let mut names = Vec::new();
    let mut place: Vec<(String, usize)> = Vec::new();
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
        if let Some(c) = declaration.captures(text) {
            names.push(Mention::declared(&c[1], "", r.row).within(&place));
            if text.ends_with('{') {
                place.push((c[1].to_owned(), 0));
            }
        } else if let Some(c) = transition.captures(text) {
            for end in [&c[1], &c[2]] {
                names.push(Mention::used(unquote(end), r.row).within(&place));
            }
        }
    }
    names
}
