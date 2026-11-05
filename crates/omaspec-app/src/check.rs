//! `render --check`: a diagram drawn now, held against the SVG committed
//! beside it, and what changed in words. The same source drawn otherwise
//! is an update's change (or the look's); another source is a diagram
//! changed without its SVG.

use std::collections::BTreeMap;

use roxmltree::{Document, Node};

/// Attributes that only color what is drawn.
const COLORS: &[&str] = &[
    "fill",
    "stroke",
    "stop-color",
    "flood-color",
    "color",
    "opacity",
    "fill-opacity",
    "stroke-opacity",
];

/// How the SVG `old` (committed) differs from `new` (drawn now from
/// `code`): `None` when they are the same, else what changed.
pub fn compare(old: &str, new: &str, code: &str) -> Option<String> {
    if old == new {
        return None;
    }
    if let Some(stored) = omaspec_core::embed::source_from_svg(old)
        && normal(&stored.code) != normal(code)
    {
        return Some("the diagram changed since this SVG was drawn".into());
    }
    let (Ok(old), Ok(new)) = (Document::parse(old), Document::parse(new)) else {
        return Some("this SVG does not read".into());
    };
    let (old, new) = (old.root_element(), new.root_element());
    let mut changes = Vec::new();
    if size(old) != size(new) {
        changes.push(format!("the picture is {}, was {}", size(new), size(old)));
    }
    let (before, after) = (words(old), words(new));
    let gone = difference(&before, &after);
    let added = difference(&after, &before);
    if !added.is_empty() {
        changes.push(format!("new words: {}", listed(&added)));
    }
    if !gone.is_empty() {
        changes.push(format!("words gone: {}", listed(&gone)));
    }
    let (old_shapes, new_shapes) = (shapes(old), shapes(new));
    if old_shapes.len() == new_shapes.len() {
        let (mut moved, mut colored) = (0, 0);
        for (a, b) in old_shapes.iter().zip(&new_shapes) {
            if a.0 != b.0 {
                moved += 1;
                continue;
            }
            let differ: Vec<&str> =
                b.1.iter()
                    .filter(|(name, value)| a.1.get(*name) != Some(value))
                    .chain(a.1.iter().filter(|(name, _)| !b.1.contains_key(*name)))
                    .map(|(name, _)| name.as_str())
                    .collect();
            if differ.is_empty() {
                continue;
            }
            if differ.iter().all(|name| COLORS.contains(name)) {
                colored += 1;
            } else {
                moved += 1;
            }
        }
        if moved > 0 {
            changes.push(format!(
                "{moved} {} moved or changed shape",
                if moved == 1 { "part" } else { "parts" }
            ));
        }
        if colored > 0 {
            changes.push(format!(
                "{colored} {} in other colors",
                if colored == 1 { "part" } else { "parts" }
            ));
        }
    } else {
        changes.push(format!(
            "{} parts, were {}",
            new_shapes.len(),
            old_shapes.len()
        ));
    }
    if changes.is_empty() {
        changes.push("drawn the same, written otherwise".into());
    }
    Some(format!("drawn differently: {}", changes.join("; ")))
}

/// `width×height` of the picture.
fn size(root: Node) -> String {
    format!(
        "{}×{}",
        root.attribute("width").unwrap_or("?"),
        root.attribute("height").unwrap_or("?")
    )
}

fn normal(text: &str) -> String {
    text.replace("\r\n", "\n").trim_end().to_owned()
}

/// The words of each `<text>`, as many times as they are drawn.
fn words(root: Node) -> BTreeMap<String, usize> {
    let mut found = BTreeMap::new();
    for text in root
        .descendants()
        .filter(|n| n.has_tag_name("text") && !in_metadata(*n))
    {
        let words: String = text
            .descendants()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect::<Vec<_>>()
            .join("");
        let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
        if !words.is_empty() {
            *found.entry(words).or_insert(0) += 1;
        }
    }
    found
}

/// What `a` has more of than `b`.
fn difference(a: &BTreeMap<String, usize>, b: &BTreeMap<String, usize>) -> Vec<String> {
    a.iter()
        .filter(|(words, n)| b.get(*words).copied().unwrap_or(0) < **n)
        .map(|(words, _)| words.clone())
        .collect()
}

fn listed(words: &[String]) -> String {
    let mut shown: Vec<String> = words.iter().take(3).map(|w| format!("“{w}”")).collect();
    if words.len() > 3 {
        shown.push(format!("{} more", words.len() - 3));
    }
    shown.join(", ")
}

/// Each element that draws, in order: its name and its attributes.
fn shapes(root: Node) -> Vec<(String, BTreeMap<String, String>)> {
    root.descendants()
        .filter(|n| n.is_element() && !in_metadata(*n))
        .map(|n| {
            let attributes = n
                .attributes()
                .map(|a| (a.name().to_owned(), a.value().to_owned()))
                .collect();
            (n.tag_name().name().to_owned(), attributes)
        })
        .collect()
}

/// Whether `node` is the stored source, which draws nothing.
fn in_metadata(node: Node) -> bool {
    node.ancestors().any(|a| a.has_tag_name("metadata"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CODE: &str = "@startuml\nA -> B : hi\n@enduml\n";

    fn svg(width: u32, words: &[&str], x: u32, fill: &str) -> String {
        let texts: String = words
            .iter()
            .map(|w| format!(r#"<text x="{x}" y="10" fill="{fill}">{w}</text>"#))
            .collect();
        let picture = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="40">{texts}<rect x="{x}" width="5" height="5" fill="{fill}"/></svg>"#
        );
        omaspec_core::embed::svg_with_source(
            &picture,
            CODE,
            omaspec_core::Language::PlantUml,
            Some("a.puml"),
        )
    }

    #[test]
    fn names_what_changed() {
        let old = svg(100, &["A", "B"], 1, "#000000");
        assert_eq!(compare(&old, &old, CODE), None);
        // Another source: the SVG was not drawn again.
        let other = "@startuml\nA -> C\n@enduml\n";
        let redrawn = old.replace("A -&gt; B : hi", "A -&gt; C");
        assert_ne!(redrawn, old);
        let stale = compare(&old, &redrawn, other).unwrap();
        assert!(stale.contains("changed since"), "{stale}");
        let moved = compare(&old, &svg(100, &["A", "B"], 2, "#000000"), CODE).unwrap();
        assert!(moved.contains("3 parts moved"), "{moved}");
        let colored = compare(&old, &svg(100, &["A", "B"], 1, "#ffffff"), CODE).unwrap();
        assert!(colored.contains("3 parts in other colors"), "{colored}");
        let words = compare(&old, &svg(120, &["A", "C"], 1, "#000000"), CODE).unwrap();
        assert!(
            words.contains("the picture is 120×40, was 100×40"),
            "{words}"
        );
        assert!(
            words.contains("new words: “C”") && words.contains("words gone: “B”"),
            "{words}"
        );
    }
}
