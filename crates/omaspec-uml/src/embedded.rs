//! A diagram inside a diagram (REQ-077). PlantUML draws what stands
//! between a line `{{` (or `{{wbs`, `{{json`, … for another kind) and a
//! line `}}` in a note, a legend or any text of several lines as a picture
//! of its own. The engine draws it the same way: as a diagram of that
//! kind, set in the text like an icon ([`crate::icons::Picture::Diagram`]),
//! standing on its line.
//!
//! It is drawn in the outer diagram's skinparams, so a look laid over the
//! outer one (the Omarchy colors, a skin pack) colors it too; its own
//! skinparams come after them. Where the engine does not draw it with
//! them, it is drawn without; where it does not draw it at all, the lines
//! stay text, as before, and lint says why. Its background is left out:
//! it stands on the note's.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::Arc;

use crate::Declined;

/// A diagram drawn inside text, as the engine drew it.
#[derive(Debug, PartialEq)]
pub struct Embedded {
    /// The engine's SVG without its root element and its background, and
    /// without the ids of its groups: what a nested `<svg>` holds.
    pub body: String,
    /// Its own coordinates: the width and height of its view box.
    pub view: (f32, f32),
    /// How large it stands in the text.
    pub width: f32,
    pub height: f32,
}

/// How deep diagrams inside diagrams are drawn; one deeper stays text.
const DEEPEST: usize = 3;
/// How many pictures a thread keeps, so a text measured again is not
/// drawn again.
const KEPT: usize = 64;

thread_local! {
    /// The skinparams of the diagram being drawn, as lines.
    static SKIN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    static DRAWN: RefCell<HashMap<String, Option<Arc<Embedded>>>> = RefCell::new(HashMap::new());
}

/// Runs `draw` with `skin`, the outer diagram's skinparams, as what a
/// diagram inside it is drawn in.
pub(crate) fn within<T>(skin: Vec<String>, draw: impl FnOnce() -> T) -> T {
    let before = SKIN.replace(skin);
    let out = draw();
    SKIN.set(before);
    out
}

/// The skinparams of a diagram's frame as lines a diagram inside it
/// starts with: those written as skinparams, not a `<style>`'s.
pub(crate) fn skin_of(frame: &crate::common::Frame) -> Vec<String> {
    frame
        .skinparams
        .iter()
        .filter(|p| p.declared.is_none() && !p.key.is_empty())
        .filter(|p| !p.key.contains(['|', '<', '>']) && !p.value.contains('\n'))
        .map(|p| format!("skinparam {} {}", p.key, p.value))
        .collect()
}

/// The kind of diagram line `line` opens: `{{` alone opens one of
/// `@startuml`, `{{wbs` a WBS; `None` when it opens none.
pub fn opens(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix("{{")?.trim();
    (rest.len() <= 12 && rest.chars().all(|c| c.is_ascii_alphabetic())).then_some(rest)
}

/// Where among `lines`, those after an opening line, the line that closes
/// it is, past the diagrams inside it.
pub fn closes(lines: &[&str]) -> Option<usize> {
    let mut depth = 0usize;
    for (at, line) in lines.iter().enumerate() {
        if opens(line).is_some() {
            depth += 1;
        } else if line.trim() == "}}" {
            if depth == 0 {
                return Some(at);
            }
            depth -= 1;
        }
    }
    None
}

/// Why the engine does not draw the diagram of `kind` written in `lines`,
/// in the outer diagram's skin; `Ok` when it does.
pub fn declined(kind: &str, lines: &[&str]) -> Result<(), Declined> {
    crate::render(&source(kind, &[], lines)).map(|_| ())
}

/// The picture of the diagram of `kind` written in `lines`; `None` when
/// the engine does not draw it, or it lies too deep.
pub(crate) fn picture(kind: &str, lines: &[&str]) -> Option<Arc<Embedded>> {
    if DEPTH.get() >= DEEPEST {
        return None;
    }
    let skin = SKIN.with_borrow(Clone::clone);
    let key = source(kind, &skin, lines);
    if let Some(kept) = DRAWN.with_borrow(|drawn| drawn.get(&key).cloned()) {
        return kept;
    }
    DEPTH.set(DEPTH.get() + 1);
    let svg = crate::render(&key).or_else(|declined| {
        if skin.is_empty() {
            Err(declined)
        } else {
            crate::render(&source(kind, &[], lines))
        }
    });
    DEPTH.set(DEPTH.get() - 1);
    let picture = svg.ok().as_deref().and_then(unwrapped).map(Arc::new);
    DRAWN.with_borrow_mut(|drawn| {
        if drawn.len() >= KEPT {
            drawn.clear();
        }
        drawn.insert(key, picture.clone());
    });
    picture
}

/// The source of the diagram of `kind` in `lines`, after `skin`.
fn source(kind: &str, skin: &[String], lines: &[&str]) -> String {
    let name = if kind.is_empty() { "uml" } else { kind };
    let mut out = format!("@start{name}\n");
    for line in skin.iter().map(String::as_str).chain(lines.iter().copied()) {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&format!("@end{name}\n"));
    out
}

/// The engine's SVG `svg` as an [`Embedded`]: its root's size and view
/// box read, the root, the background and the ids left out.
fn unwrapped(svg: &str) -> Option<Embedded> {
    let open = svg.find('>')?;
    let root = &svg[..open];
    let attr = |name: &str| -> Option<&str> {
        root.split(&format!(" {name}=\"")).nth(1)?.split('"').next()
    };
    let width: f32 = attr("width")?.parse().ok()?;
    let height: f32 = attr("height")?.parse().ok()?;
    let view: Vec<f32> = attr("viewBox")?
        .split_whitespace()
        .filter_map(|n| n.parse().ok())
        .collect();
    let [_, _, vw, vh] = view[..] else {
        return None;
    };
    let mut body = svg[open + 1..].strip_suffix("</svg>")?;
    // The background is the page's; here the note's is.
    if body.starts_with(r#"<rect x="0" y="0""#)
        && let Some(end) = body.find("/>")
    {
        body = &body[end + 2..];
    }
    Some(Embedded {
        body: without_ids(body),
        view: (vw, vh),
        width,
        height,
    })
}

/// `body` without its `id` attributes: the outer picture's groups have
/// the same names, and the pane finds its lines and parts by them.
fn without_ids(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find(r#" id=""#) {
        out.push_str(&rest[..at]);
        let after = &rest[at + 5..];
        match after.find('"') {
            Some(end) => rest = &after[end + 1..],
            None => {
                rest = after;
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_opens_a_kind_and_a_later_one_closes_it() {
        assert_eq!(opens("  {{"), Some(""));
        assert_eq!(opens("{{wbs"), Some("wbs"));
        assert_eq!(opens("{{ x }}"), None);
        assert_eq!(opens("{{json5"), None);
        // A diagram inside the inner one closes first.
        assert_eq!(closes(&["a", "{{", "b", "}}", " }}", "c"]), Some(4));
        assert_eq!(closes(&["a", "b"]), None);
    }

    #[test]
    fn a_picture_is_the_engine_s_without_its_root_background_and_ids() {
        let drawn = picture("", &["Bob -> Carol : hi"]).expect("drawn");
        assert!(drawn.width > 0.0 && drawn.height > 0.0);
        assert!(!drawn.body.contains("<svg") && !drawn.body.contains(" id=\""));
        assert!(drawn.body.contains(">hi</tspan>"), "{}", drawn.body);
        // What the engine does not draw is no picture.
        assert!(picture("nosuchkind", &["x"]).is_none());
    }

    #[test]
    fn a_note_draws_the_diagram_inside_it_and_keeps_text_it_cannot() {
        let drawn = crate::render(
            "@startuml\nA -> B\nnote right\n  before\n  {{\n  B -> C : inside\n  }}\n  after\nend note\n@enduml\n",
        )
        .unwrap();
        assert!(drawn.contains("<svg x="), "a picture in the note: {drawn}");
        assert!(drawn.contains(">inside</tspan>") && !drawn.contains(">{{</tspan>"));
        assert!(drawn.contains(">before</tspan>") && drawn.contains(">after</tspan>"));
        // A kind the engine leaves to the Java PlantUML stays text.
        let text = crate::render(
            "@startuml\nA -> B\nnote right\n{{dot\ndigraph { a -> b }\n}}\nend note\n@enduml\n",
        )
        .unwrap();
        assert!(
            !text.contains("<svg x=") && text.contains("{{dot"),
            "{text}"
        );
    }

    #[test]
    fn a_diagram_inside_takes_the_skin_of_the_one_around_it() {
        let svg = crate::render(
            "@startuml\nskinparam participantBackgroundColor #123456\nA -> B\nnote right\n{{\nC -> D\n}}\nend note\n@enduml\n",
        )
        .unwrap();
        let inside = &svg[svg.find("<svg x=").expect("the picture")..];
        assert!(inside.to_ascii_lowercase().contains("#123456"), "{inside}");
    }

    #[test]
    fn only_so_deep() {
        // Each level a note with a diagram inside; the deepest stays text.
        let mut lines = vec!["A -> B".to_owned()];
        for _ in 0..DEEPEST + 1 {
            let mut outer = vec![
                "A -> B".to_owned(),
                "note right".to_owned(),
                "{{".to_owned(),
            ];
            outer.extend(lines);
            outer.extend(["}}".to_owned(), "end note".to_owned()]);
            lines = outer;
        }
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let drawn = picture("", &refs).expect("drawn");
        assert!(drawn.body.contains("{{"), "the deepest is text");
    }
}
