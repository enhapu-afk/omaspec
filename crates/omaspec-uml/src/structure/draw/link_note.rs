//! A note on a line (`note on link`), as PlantUML draws one: the line's
//! words and the note one block, the note under the words where no side
//! is written, beside or over them where one is, in their place where the
//! line has none. The layout keeps the room of the block as the line's
//! words; the two are drawn in it.

use super::{NOTE_PAD, dog_ear, fill_of, legible_on, note_size};
use crate::Declined;
use crate::color::{self, Paint};
use crate::graph::Rect;
use crate::structure::skin::Skin;
use crate::structure::{LinkNote, Side};
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{Block, Family};

/// Between a line's words and its note.
const GAP: f32 = 4.0;

/// A note on a line, measured beside the line's words.
#[derive(Debug, Clone)]
pub(super) struct Drawn {
    /// The line's own words, and where they stand in the room.
    words: Block,
    words_at: (f32, f32),
    /// The note's box in the room, and its words.
    note: Rect,
    text: Block,
    fill: Paint,
    gradient: Option<color::Gradient>,
    border: Stroke,
}

/// The room a line's `words` take with `note`, as the layout keeps it,
/// and the two in it.
pub(super) fn measure(
    note: &LinkNote,
    words: Block,
    skin: &Skin,
    family: Family,
) -> Result<(Block, Drawn), Declined> {
    let (fill, gradient) = match &note.fill {
        Some(value) => fill_of(value, note.text.line)?,
        None => (skin.note.fill, skin.note.gradient),
    };
    // Words on a fill of the writer's are moved toward black or white
    // until they read, as on any note of a fill of its own.
    let mut font = skin.note.font;
    if note.fill.is_some() {
        legible_on(&mut font, &color::grounds(fill, gradient));
    }
    let text = note.text.measure_written(font, family)?;
    let (nw, nh) = note_size(&text);
    let (lw, lh) = (words.width, words.height);
    let gap = if words.is_empty() { 0.0 } else { GAP };
    let ((w, h), words_at, note_at) = match note.side {
        None | Some(Side::Bottom) => {
            let w = lw.max(nw);
            (
                (w, lh + gap + nh),
                ((w - lw) / 2.0, 0.0),
                ((w - nw) / 2.0, lh + gap),
            )
        }
        Some(Side::Top) => {
            let w = lw.max(nw);
            (
                (w, nh + gap + lh),
                ((w - lw) / 2.0, nh + gap),
                ((w - nw) / 2.0, 0.0),
            )
        }
        Some(Side::Right) => {
            let h = lh.max(nh);
            (
                (lw + gap + nw, h),
                (0.0, (h - lh) / 2.0),
                (lw + gap, (h - nh) / 2.0),
            )
        }
        Some(Side::Left) => {
            let h = lh.max(nh);
            (
                (nw + gap + lw, h),
                (nw + gap, (h - lh) / 2.0),
                (0.0, (h - nh) / 2.0),
            )
        }
    };
    let room = Block {
        width: w,
        height: h,
        ..Block::empty()
    };
    Ok((
        room,
        Drawn {
            words,
            words_at,
            note: Rect {
                x: note_at.0,
                y: note_at.1,
                w: nw,
                h: nh,
            },
            text,
            fill,
            gradient,
            border: Stroke::new(skin.note.border, skin.note.thickness),
        },
    ))
}

/// The line's words and its note in the room the layout gave the words,
/// at `at`.
pub(super) fn paint(canvas: &mut Canvas, drawn: &Drawn, at: (f32, f32)) {
    let (x, y) = at;
    canvas.text(
        Layer::Front,
        &drawn.words,
        (x + drawn.words_at.0, y + drawn.words_at.1),
        Align::Left,
    );
    let rect = Rect {
        x: x + drawn.note.x,
        y: y + drawn.note.y,
        ..drawn.note
    };
    canvas.gradient_over(drawn.gradient, (rect.x, rect.y, rect.w, rect.h), |canvas| {
        dog_ear(canvas, rect, drawn.fill, drawn.border);
    });
    canvas.text(
        Layer::Front,
        &drawn.text,
        (rect.x + NOTE_PAD, rect.y + NOTE_PAD),
        Align::Left,
    );
}

#[cfg(test)]
mod tests {
    use crate::structure::Side;

    fn diagram(body: &str) -> crate::structure::Diagram {
        let source = format!("@startuml\n{body}\n@enduml\n");
        let (_, lines) = crate::source::body(&source).unwrap();
        crate::structure::parse(&lines).unwrap_or_else(|err| panic!("{err:?}"))
    }

    /// `note on link` is on the line written last, its side, its color
    /// and its words as written, one line or a block; a second takes the
    /// first one's place, as in PlantUML.
    #[test]
    fn a_note_is_on_the_line_written_last() {
        let d = diagram("class A\nclass B\nA --> B : l\nnote right on link #pink : n");
        let note = d.root.edges[0].note.as_ref().unwrap();
        assert_eq!(
            (note.side, note.fill.as_deref(), note.text.value.as_str()),
            (Some(Side::Right), Some("#pink"), "n")
        );
        let d = diagram("A --> B\nnote on link\n  x\n  y\nend note");
        let note = d.root.edges[0].note.as_ref().unwrap();
        assert_eq!((note.side, note.text.value.as_str()), (None, "x\ny"));
        let d = diagram("A --> B\nnote on link : first\nnote bottom on link : second");
        let note = d.root.edges[0].note.as_ref().unwrap();
        assert_eq!(note.text.value, "second");
        // Between packages too.
        let d = diagram("package p {\n  class A\n}\nA --> B\nnote on link : n");
        assert!(d.root.crossings[0].arrow.note.is_some());
        for (body, why) in [
            ("class A\nnote on link : n", "line before it"),
            ("A -[hidden]-> B\nnote on link : n", "hidden"),
        ] {
            let source = format!("@startuml\n{body}\n@enduml\n");
            let (_, lines) = crate::source::body(&source).unwrap();
            let err = crate::structure::parse(&lines).unwrap_err();
            assert!(err.reason.contains(why), "{err:?}");
        }
    }

    /// The note stands under the line's words, or on the side written;
    /// its words are tied to their line.
    #[test]
    fn the_note_stands_by_the_words() {
        let at = |svg: &str, word: &str| -> (f32, f32) {
            let at = svg
                .find(&format!(">{word}</tspan>"))
                .unwrap_or_else(|| panic!("{svg}"));
            let tag = &svg[svg[..at].rfind("<tspan").unwrap()..at];
            let number = |attr: &str| -> f32 {
                let start = tag.find(&format!(" {attr}=\"")).unwrap() + attr.len() + 3;
                tag[start..].split('"').next().unwrap().parse().unwrap()
            };
            (number("x"), number("y"))
        };
        let below =
            crate::render("@startuml\nclass A\nA --> B : words\nnote on link : under\n@enduml\n")
                .unwrap();
        assert!(at(&below, "under").1 > at(&below, "words").1, "{below}");
        let beside = crate::render(
            "@startuml\nclass A\nA --> B : words\nnote right on link : beside\n@enduml\n",
        )
        .unwrap();
        assert!(at(&beside, "beside").0 > at(&beside, "words").0, "{beside}");
        let over = crate::render(
            "@startuml\nclass A\nA --> B : words\nnote top on link : over\n@enduml\n",
        )
        .unwrap();
        assert!(at(&over, "over").1 < at(&over, "words").1, "{over}");
        // On a fill of its own the words read.
        let dark =
            crate::render("@startuml\nclass A\nA --> B\nnote on link #blue : dark\n@enduml\n")
                .unwrap();
        let word = dark.find(">dark<").unwrap();
        let tag = &dark[dark[..word].rfind("<tspan").unwrap()..word];
        assert!(!tag.contains("fill=\"#000000\""), "{tag}");
    }
}
