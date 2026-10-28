//! What is drawn at a point of a diagram: the label there, or the label of
//! the box, diamond or frame around the point, for jumping to its source.

use crate::export::usvg::{self, Node, Rect, Tree};

/// A label of the diagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// Its text, lines joined by spaces.
    pub text: String,
    /// How many labels with the same text come before it in the drawing.
    pub nth: usize,
}

/// A click this close to a label still hits it, in diagram units.
const SLACK: f32 = 3.0;
/// Shapes larger than this share of the drawing are backgrounds, not boxes.
const BACKGROUND: f32 = 0.5;
/// How far a label with no box of its own grows, in diagram units.
const PAD: f32 = 4.0;

struct Text {
    text: String,
    rect: Rect,
}

/// The label at `x`, `y` (diagram units): the label there, else the one in
/// the smallest shape there.
pub fn label_at(tree: &Tree, x: f32, y: f32) -> Option<Label> {
    let mut texts = Vec::new();
    let mut shapes = Vec::new();
    collect(tree.root(), &mut texts, &mut shapes);

    let hit = |rect: &Rect, slack: f32| {
        x >= rect.left() - slack
            && x <= rect.right() + slack
            && y >= rect.top() - slack
            && y <= rect.bottom() + slack
    };
    let area = |rect: &Rect| rect.width() * rect.height();

    let direct = texts
        .iter()
        .enumerate()
        .filter(|(_, t)| hit(&t.rect, SLACK))
        .min_by(|a, b| area(&a.1.rect).total_cmp(&area(&b.1.rect)))
        .map(|(i, _)| i);
    let index = direct.or_else(|| {
        let drawing = tree.size().width() * tree.size().height();
        let shape = shapes
            .iter()
            .filter(|s| hit(s, 0.0) && area(s) < drawing * BACKGROUND)
            .min_by(|a, b| area(a).total_cmp(&area(b)))?;
        // The first label whose middle lies in the shape.
        texts.iter().position(|t| {
            let (cx, cy) = (
                t.rect.left() + t.rect.width() / 2.0,
                t.rect.top() + t.rect.height() / 2.0,
            );
            cx >= shape.left() && cx <= shape.right() && cy >= shape.top() && cy <= shape.bottom()
        })
    })?;
    let text = texts[index].text.clone();
    let nth = texts[..index].iter().filter(|t| t.text == text).count();
    Some(Label { text, nth })
}

/// Every label's text, in the order it is drawn. Repeated names stay
/// repeated, so the nth copy is the nth entry with that text.
pub fn texts(tree: &Tree) -> Vec<String> {
    let mut texts = Vec::new();
    collect(tree.root(), &mut texts, &mut Vec::new());
    texts.into_iter().map(|text| text.text).collect()
}

/// The place of the `nth` label (from 0) whose text is exactly `text`:
/// `(x, y, width, height)` in diagram units. It is the smallest filled
/// shape that holds the middle of that label. A shape larger than half
/// the drawing is a background and does not count. With no such shape
/// (an arrow's words, a port) the label's own box grows by a few units.
pub fn spot(tree: &Tree, text: &str, nth: usize) -> Option<(f32, f32, f32, f32)> {
    let mut texts = Vec::new();
    let mut shapes = Vec::new();
    collect(tree.root(), &mut texts, &mut shapes);
    let mut seen = 0;
    let label = texts.iter().find(|label| {
        if label.text != text {
            return false;
        }
        if seen == nth {
            return true;
        }
        seen += 1;
        false
    })?;
    let (cx, cy) = (
        label.rect.left() + label.rect.width() / 2.0,
        label.rect.top() + label.rect.height() / 2.0,
    );
    let area = |rect: &Rect| rect.width() * rect.height();
    let drawing = tree.size().width() * tree.size().height();
    let held = |rect: &Rect| {
        cx >= rect.left() && cx <= rect.right() && cy >= rect.top() && cy <= rect.bottom()
    };
    if let Some(shape) = shapes
        .iter()
        .filter(|shape| held(shape) && area(shape) < drawing * BACKGROUND)
        .min_by(|a, b| area(a).total_cmp(&area(b)))
    {
        return Some((shape.left(), shape.top(), shape.width(), shape.height()));
    }
    let rect = &label.rect;
    Some((
        rect.left() - PAD,
        rect.top() - PAD,
        rect.width() + PAD * 2.0,
        rect.height() + PAD * 2.0,
    ))
}

fn collect(group: &usvg::Group, texts: &mut Vec<Text>, shapes: &mut Vec<Rect>) {
    for node in group.children() {
        match node {
            Node::Group(g) => collect(g, texts, shapes),
            Node::Text(t) => {
                let text = t
                    .chunks()
                    .iter()
                    .map(|c| c.text())
                    .collect::<Vec<_>>()
                    .join(" ");
                let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                if !text.is_empty() {
                    texts.push(Text {
                        text,
                        rect: t.abs_bounding_box(),
                    });
                }
            }
            Node::Path(p) if p.is_visible() && p.fill().is_some() => {
                if let Some(rect) = p.abs_bounding_box().to_non_zero_rect() {
                    shapes.push(rect.to_rect());
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export;

    /// Where `text` is drawn: the middle of its first label.
    fn middle(tree: &Tree, text: &str) -> (f32, f32) {
        let mut texts = Vec::new();
        collect(tree.root(), &mut texts, &mut Vec::new());
        let t = texts.iter().find(|t| t.text == text).expect(text);
        (
            t.rect.left() + t.rect.width() / 2.0,
            t.rect.top() + t.rect.height() / 2.0,
        )
    }

    const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="200">
        <rect x="0" y="0" width="400" height="200" fill="white"/>
        <rect x="10" y="10" width="120" height="60" fill="#eee"/>
        <text x="20" y="45" font-family="sans-serif" font-size="14">Edit</text>
        <rect x="200" y="10" width="120" height="60" fill="#eee"/>
        <text x="210" y="45" font-family="sans-serif" font-size="14">Edit</text>
        <text x="20" y="150" font-family="sans-serif" font-size="14"><tspan x="20" y="150">two</tspan><tspan x="20" y="170">lines</tspan></text>
    </svg>"##;

    #[test]
    fn finds_the_label_and_counts_twins() {
        let tree = export::parse(SVG).unwrap();
        let (x, y) = middle(&tree, "Edit");
        assert_eq!(
            label_at(&tree, x, y),
            Some(Label {
                text: "Edit".into(),
                nth: 0
            })
        );
        // The second box: by its label, and by its empty corner.
        assert_eq!(label_at(&tree, 230.0, 40.0).unwrap().nth, 1);
        assert_eq!(label_at(&tree, 315.0, 15.0).unwrap().nth, 1);
        // Lines of one text come as one label.
        let (x, y) = middle(&tree, "two lines");
        assert_eq!(label_at(&tree, x, y).unwrap().text, "two lines");
        // The background is no box.
        assert_eq!(label_at(&tree, 380.0, 190.0), None);
    }

    #[test]
    fn the_nth_label_owns_the_smallest_shape() {
        let tree = export::parse(SVG).unwrap();
        let (x, _, w, _) = spot(&tree, "Edit", 0).unwrap();
        let (x2, _, w2, _) = spot(&tree, "Edit", 1).unwrap();
        assert!(x < x2, "{x} then {x2}");
        assert!((x - 10.0).abs() < 1.0 && (w - 120.0).abs() < 1.0, "{x} {w}");
        assert!((w2 - 120.0).abs() < 1.0, "{w2}");
        assert!(spot(&tree, "Edit", 2).is_none());
        assert!(spot(&tree, "Editing", 0).is_none());
        // No box of its own: the words, grown a little, not the page.
        let (x, y, w, h) = spot(&tree, "two lines", 0).unwrap();
        assert!(w < 200.0 && h < 200.0, "{w}×{h}");
        assert!(x < 40.0 && y > 100.0, "{x} {y}");
    }

    #[test]
    fn finds_labels_in_a_mermaid_diagram() {
        let svg = crate::mermaid::render(
            "flowchart LR\n  A[Edit] --> B{Saved?}\n  B -->|yes| C[Render]\n",
        )
        .unwrap();
        let tree = export::parse(&svg).unwrap();
        for text in ["Edit", "Saved?", "yes", "Render"] {
            let (x, y) = middle(&tree, text);
            assert_eq!(label_at(&tree, x, y).unwrap().text, text);
        }
    }
}
