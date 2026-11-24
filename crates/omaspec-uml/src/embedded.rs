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

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

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
pub const DEEPEST: usize = 3;
/// How many pictures a thread keeps, so a text measured again is not
/// drawn again.
const KEPT: usize = 64;

/// What a thread draws diagrams inside a diagram with.
#[derive(Default)]
struct State {
    /// The skinparams of the PlantUML diagram being drawn, as lines;
    /// `None` outside one: Mermaid writes `{{` as text.
    skin: Option<Vec<String>>,
    /// How deep in diagrams inside diagrams the drawing is.
    depth: usize,
    drawn: HashMap<String, Option<Arc<Embedded>>>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Puts back what it holds when dropped, also when a drawing panics and
/// the thread draws on.
struct Restore {
    skin: Option<Option<Vec<String>>>,
    deeper: bool,
}

impl Drop for Restore {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|state| {
            if let Some(skin) = self.skin.take() {
                state.skin = skin;
            }
            if self.deeper {
                state.depth -= 1;
            }
        });
    }
}

/// Runs `draw`, a PlantUML diagram's drawing, with `skin`, its
/// skinparams, as what a diagram inside it is drawn in.
pub(crate) fn within<T>(skin: Vec<String>, draw: impl FnOnce() -> T) -> T {
    let before = STATE.with_borrow_mut(|state| state.skin.replace(skin));
    let _restore = Restore {
        skin: Some(before),
        deeper: false,
    };
    draw()
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

/// Why the engine writes the diagram of `kind` in `lines`, inside `depth`
/// diagrams, as text; `Ok` when it draws it, as [`picture`] does.
pub fn drawn(kind: &str, lines: &[&str], depth: usize) -> Result<(), String> {
    if depth >= DEEPEST {
        return Err(format!("it lies more than {DEEPEST} diagrams deep"));
    }
    let svg = crate::render(&source(kind, &[], lines)).map_err(|declined| declined.reason)?;
    unwrapped(&svg)
        .map(|_| ())
        .ok_or_else(|| "its picture cannot be set in the text".to_owned())
}

/// The picture of the diagram of `kind` written in `lines`; `None` when
/// the engine does not draw it, it lies too deep, or the text around it is
/// no PlantUML diagram's.
pub(crate) fn picture(kind: &str, lines: &[&str]) -> Option<Arc<Embedded>> {
    let (skin, depth) = STATE.with_borrow(|state| (state.skin.clone(), state.depth));
    let skin = skin?;
    if depth >= DEEPEST {
        return None;
    }
    let written = source(kind, &skin, lines);
    // What lies deeper inside it is drawn or not by how deep it is.
    let key = format!("{depth}\n{written}");
    if let Some(kept) = STATE.with_borrow(|state| state.drawn.get(&key).cloned()) {
        return kept;
    }
    let svg = {
        STATE.with_borrow_mut(|state| state.depth += 1);
        let _restore = Restore {
            skin: None,
            deeper: true,
        };
        crate::render(&written).or_else(|declined| {
            if skin.is_empty() {
                Err(declined)
            } else {
                crate::render(&source(kind, &[], lines))
            }
        })
    };
    let picture = svg.ok().as_deref().and_then(unwrapped).map(Arc::new);
    STATE.with_borrow_mut(|state| {
        if state.drawn.len() >= KEPT {
            state.drawn.clear();
        }
        state.drawn.insert(key, picture.clone());
    });
    picture
}

/// A diagram of `kind` drawn from `lines` as [`picture`] draws it, also
/// outside a drawing, or why it is not: what the engine says of it, or
/// that it lies too deep inside others.
pub(crate) fn picture_or_why(kind: &str, lines: &[&str]) -> Result<Arc<Embedded>, String> {
    let outside = STATE.with_borrow(|state| state.skin.is_none());
    let drawn = match outside {
        true => within(Vec::new(), || picture(kind, lines)),
        false => picture(kind, lines),
    };
    drawn.ok_or_else(|| {
        if STATE.with_borrow(|state| state.depth) >= DEEPEST {
            return "a diagram this deep inside others is not drawn".to_owned();
        }
        match crate::render(&source(kind, &[], lines)) {
            Err(declined) => declined.reason,
            Ok(_) => "it cannot be drawn here".to_owned(),
        }
    })
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
    // The background is the page's; here the note's is. Definitions may
    // stand before it.
    let mut kept = String::new();
    while body.starts_with("<defs>")
        && let Some(end) = body.find("</defs>")
    {
        kept.push_str(&body[..end + 7]);
        body = &body[end + 7..];
    }
    if body.starts_with(r#"<rect x="0" y="0""#)
        && let Some(end) = body.find("/>")
    {
        body = &body[end + 2..];
    }
    let body = format!("{kept}{body}");
    let body = body.as_str();
    Some(Embedded {
        body: without_ids(body),
        view: (vw, vh),
        width,
        height,
    })
}

/// `body` without its `id` attributes: the outer picture's groups have
/// the same names, and the pane finds its lines and parts by them. A
/// gradient keeps its id, which its colors and its way name, so it is the
/// same wherever it is defined.
fn without_ids(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find(r#" id=""#) {
        out.push_str(&rest[..at]);
        let after = &rest[at + 5..];
        if after.starts_with("omaspec-gradient-") {
            out.push_str(r#" id=""#);
            rest = after;
            continue;
        }
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
        let drawn = within(Vec::new(), || picture("", &["Bob -> Carol : hi"])).expect("drawn");
        assert!(drawn.width > 0.0 && drawn.height > 0.0);
        assert!(!drawn.body.contains("<svg") && !drawn.body.contains(" id=\""));
        assert!(drawn.body.contains(">hi</tspan>"), "{}", drawn.body);
        // What the engine does not draw is no picture.
        assert!(within(Vec::new(), || picture("nosuchkind", &["x"])).is_none());
        // Outside a PlantUML diagram there is none: Mermaid writes `{{`.
        assert!(picture("", &["Bob -> Carol : hi"]).is_none());
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
    fn a_note_with_a_diagram_inside_stands_on_all_its_lines() {
        // `before`, `{{`, `B -> C`, `}}`, `after`: five lines from line 3.
        let source =
            "@startuml\nA -> B\nnote right\nbefore\n{{\nB -> C\n}}\nafter\nend note\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let note = map
            .tied
            .iter()
            .find(|tied| tied.line == 3)
            .expect("the note's words");
        assert_eq!(note.span, 5, "{:?}", map.tied);
    }

    #[test]
    fn a_note_whose_diagram_is_never_closed_ends_where_it_ends() {
        // The `}}` belongs to the next note; the first ends at its own end.
        let svg = crate::render(
            "@startuml\nA -> B\nnote right\n{{\nend note\nB -> C : between\nnote left\n}}\nend note\n@enduml\n",
        )
        .unwrap();
        assert!(svg.contains(">between</tspan>"), "{svg}");
        assert!(svg.contains(">C</tspan>"), "{svg}");
    }

    #[test]
    fn a_mermaid_note_writes_its_braces() {
        let svg = crate::sequence::from_mermaid(
            "sequenceDiagram\nA->>B: hi\nNote right of B: {{<br/>C -> D : leak<br/>}}\n",
        )
        .map(|d| crate::sequence::draw(&d).unwrap())
        .unwrap();
        assert!(!svg.contains("<svg x="), "{svg}");
    }

    #[test]
    fn a_panic_inside_leaves_nothing_behind() {
        let _ = std::panic::catch_unwind(|| {
            within(vec!["skinparam x y".to_owned()], || {
                STATE.with_borrow_mut(|state| state.depth += 1);
                let _restore = Restore {
                    skin: None,
                    deeper: true,
                };
                panic!("a drawing that fails");
            })
        });
        STATE.with_borrow(|state| assert!(state.skin.is_none() && state.depth == 0));
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
        let drawn = within(Vec::new(), || picture("", &refs)).expect("drawn");
        assert!(drawn.body.contains("{{"), "the deepest is text");
    }
}
