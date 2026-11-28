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
/// A shape whose edges lie this close to the page's, in diagram units, is
/// the background. A container drawn on the page keeps its margin.
const PAGE_EDGE: f32 = 2.0;
/// A shape with no stroke that hugs a label this closely, on every side,
/// is that label's ground, laid under it to cover a line. It is no box.
const SNUG: f32 = 4.0;
/// How far a label with no box of its own grows, in diagram units.
const PAD: f32 = 4.0;
/// Stacked lines of one title sit within this of each other. A container's
/// body starts further down, so a label there does not name the box.
const HEADER_GAP: f32 = 6.0;

struct Text {
    text: String,
    rect: Rect,
    /// The letters' own color, red green blue, each 0 to 1.
    ink: [f32; 3],
}

struct Shape {
    rect: Rect,
    /// The fill, when it is a plain color. A pattern has none.
    fill: Option<[f32; 3]>,
    /// Whether the shape has an outline of its own.
    stroked: bool,
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
        let page = tree.size();
        let shape = shapes
            .iter()
            .filter(|s| hit(&s.rect, 0.0) && !covers_page(&s.rect, page))
            .min_by(|a, b| area(&a.rect).total_cmp(&area(&b.rect)))?;
        // The shape's own topmost label, its name. A label of a box nested
        // in it is not the shape's, nor is one that only lies on its ground.
        texts
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                let (cx, cy) = center(&t.rect);
                holds(&shape.rect, cx, cy)
                    && around(&shapes, &t.rect, page)
                        .is_none_or(|found| std::ptr::eq(found, &shape.rect))
            })
            .min_by(|a, b| a.1.rect.top().total_cmp(&b.1.rect.top()))
            .map(|(i, _)| i)
    })?;
    let text = texts[index].text.clone();
    let nth = texts[..index].iter().filter(|t| t.text == text).count();
    Some(Label { text, nth })
}

/// Where the words of each label that reads exactly `text` are drawn, in
/// the order drawn: `(x, y, width, height)` of the words themselves, not
/// of a box they name.
pub fn words(tree: &Tree, text: &str) -> Vec<(f32, f32, f32, f32)> {
    let mut texts = Vec::new();
    collect(tree.root(), &mut texts, &mut Vec::new());
    texts
        .iter()
        .filter(|label| label.text == text)
        .map(|label| {
            let r = &label.rect;
            (r.left(), r.top(), r.width(), r.height())
        })
        .collect()
}

/// Every label's text, in the order it is drawn. Repeated names stay
/// repeated, so the nth copy is the nth entry with that text.
pub fn texts(tree: &Tree) -> Vec<String> {
    let mut texts = Vec::new();
    collect(tree.root(), &mut texts, &mut Vec::new());
    texts.into_iter().map(|text| text.text).collect()
}

/// Where a label sits. A box is a shape the label names. Words are a label
/// with no box of its own, or one that only lies inside a larger box: a
/// message, or the text on a line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seat {
    Shape {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Words {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        /// The letters' color, red green blue, each 0 to 1.
        ink: [f32; 3],
        /// The fill under the letters, when the picture paints one.
        /// Nothing here means the letters sit on the theme's paper.
        ground: Option<[f32; 3]>,
    },
}

impl Seat {
    /// `(x, y, width, height)` in diagram units.
    pub fn bounds(self) -> (f32, f32, f32, f32) {
        match self {
            Self::Shape { x, y, w, h } | Self::Words { x, y, w, h, .. } => (x, y, w, h),
        }
    }
}

/// Where the `nth` label (from 0) whose text is exactly `text` sits.
/// A shape that covers the page is a background and does not count.
pub fn seat(tree: &Tree, text: &str, nth: usize) -> Option<Seat> {
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
    let (cx, cy) = center(&label.rect);
    let page = tree.size();
    if let Some(shape) = around(&shapes, &label.rect, page)
        && names_the_shape(label, shape, &texts, &shapes, page)
    {
        return Some(Seat::Shape {
            x: shape.left(),
            y: shape.top(),
            w: shape.width(),
            h: shape.height(),
        });
    }
    let rect = &label.rect;
    (rect.width() > 0.0 && rect.height() > 0.0).then_some(Seat::Words {
        x: rect.left(),
        y: rect.top(),
        w: rect.width(),
        h: rect.height(),
        ink: label.ink,
        ground: ground_at(&shapes, cx, cy),
    })
}

/// How far a label may stick out of the shape that holds it, in diagram units.
const FIT: f32 = 4.0;

/// The smallest shape the letters actually sit in. A bar they only cross,
/// such as a lifeline or its activation, does not count: the letters have
/// to fit inside it. Neither does a shape that covers the page, nor the
/// label's own ground.
fn around<'a>(shapes: &'a [Shape], label: &Rect, page: usvg::Size) -> Option<&'a Rect> {
    let area = |rect: &Rect| rect.width() * rect.height();
    shapes
        .iter()
        .filter(|shape| {
            fits(&shape.rect, label) && !covers_page(&shape.rect, page) && !ground_of(shape, label)
        })
        .min_by(|a, b| area(&a.rect).total_cmp(&area(&b.rect)))
        .map(|shape| &shape.rect)
}

/// Whether `shape` is the page itself, rather than a box drawn on it: it
/// reaches every edge of the page.
fn covers_page(shape: &Rect, page: usvg::Size) -> bool {
    let (w, h) = (page.width(), page.height());
    w > 0.0
        && h > 0.0
        && shape.left() <= PAGE_EDGE
        && shape.top() <= PAGE_EDGE
        && shape.right() >= w - PAGE_EDGE
        && shape.bottom() >= h - PAGE_EDGE
}

/// Whether `shape` is only the ground under `label`: no outline, and no
/// room around the letters. A box keeps its padding and its stroke.
fn ground_of(shape: &Shape, label: &Rect) -> bool {
    !shape.stroked
        && shape.rect.width() <= label.width() + 2.0 * SNUG
        && shape.rect.height() <= label.height() + 2.0 * SNUG
}

fn fits(shape: &Rect, label: &Rect) -> bool {
    let (cx, cy) = center(label);
    holds(shape, cx, cy)
        && shape.width() + FIT >= label.width()
        && shape.height() + FIT >= label.height()
}

/// The shapes of a part of a picture, read once, to ask [`Fills::of`]
/// and [`Fills::bend`] for its boxes ([`fills_of`]).
#[derive(Debug, Clone, Default)]
pub struct Fills(Vec<Outline>);

/// A shape as [`Fills`] knows it.
#[derive(Debug, Clone)]
struct Outline {
    rect: Rect,
    /// Its fill, when it is a plain color.
    fill: Option<[f32; 3]>,
    /// How its outline bends, when it is a box ([`bend_of`]).
    bend: Option<Bend>,
}

/// How a box's outline lies in the rectangle around it, in diagram units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bend {
    /// How far its corners are rounded, or cut off.
    pub radius: f32,
    /// How far in from that rectangle its sides run at most: a step in a
    /// side, a hand-drawn side's shake, or a margin it is drawn with.
    pub inset: f32,
}

/// The shapes of `groups` alone with a fill or an outline, and how they
/// are filled: the groups of one part of a picture, so that what is asked
/// of a box is answered by its own outline and never by another's
/// (docs/adr/0029).
pub fn fills_of<'a>(groups: impl IntoIterator<Item = &'a usvg::Group>) -> Fills {
    let mut outlines = Vec::new();
    for group in groups {
        collect_outlines(group, &mut outlines);
    }
    Fills(outlines)
}

fn collect_outlines(group: &usvg::Group, outlines: &mut Vec<Outline>) {
    for node in group.children() {
        match node {
            Node::Group(g) => collect_outlines(g, outlines),
            Node::Path(path) => {
                let filled = path.fill().filter(|fill| fill.opacity().get() > 0.0);
                let stroked = path.stroke().is_some_and(|s| s.opacity().get() > 0.0);
                if !path.is_visible() || (filled.is_none() && !stroked) {
                    continue;
                }
                if let Some(rect) = path.abs_bounding_box().to_non_zero_rect() {
                    outlines.push(Outline {
                        rect: rect.to_rect(),
                        fill: filled.and_then(|fill| paint_rgb(fill.paint())),
                        bend: bend_of(path),
                    });
                }
            }
            _ => {}
        }
    }
}

impl Fills {
    /// The plain colors its shapes are filled with, each as often as a
    /// shape has it.
    pub fn colors(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.0.iter().filter_map(|outline| outline.fill)
    }

    /// The fill of the shape drawn on the box `(x, y, w, h)`, when one
    /// sits there. A sketch's stroke bows, so the shape may miss the box
    /// by a little.
    pub fn of(&self, x: f32, y: f32, w: f32, h: f32) -> Option<[f32; 3]> {
        self.nearest(x, y, w, h, |outline| outline.fill.is_some())
            .and_then(|outline| outline.fill)
    }

    /// How the outline of the box `(x, y, w, h)` bends, filled or only
    /// outlined, its margin within that box counted in: `None` when its
    /// shape is no box, as a diamond, a circle or a hexagon is not
    /// ([`bend_of`]), or none is drawn there.
    pub fn bend(&self, x: f32, y: f32, w: f32, h: f32) -> Option<Bend> {
        let outline = self.nearest(x, y, w, h, |_| true)?;
        let bend = outline.bend?;
        let rect = &outline.rect;
        let margin = (rect.left() - x)
            .max(rect.top() - y)
            .max(x + w - rect.right())
            .max(y + h - rect.bottom())
            .max(0.0);
        Some(Bend {
            inset: bend.inset + margin,
            ..bend
        })
    }

    fn nearest(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        counts: impl Fn(&Outline) -> bool,
    ) -> Option<&Outline> {
        let mut best: Option<(&Outline, f32)> = None;
        for outline in self.0.iter().filter(|outline| counts(outline)) {
            let rect = &outline.rect;
            let delta = (rect.left() - x).abs()
                + (rect.top() - y).abs()
                + (rect.width() - w).abs()
                + (rect.height() - h).abs();
            if delta > 48.0 {
                continue;
            }
            if best.is_none_or(|(_, have)| delta < have) {
                best = Some((outline, delta));
            }
        }
        best.map(|(outline, _)| outline)
    }
}

/// How `path` bends, in diagram units, when it is a box. Its sides run
/// across and down, near enough for the hand-drawn look's bowed and shaken
/// ones, and it turns from one to the next in its corners, by at most a
/// third of its shorter side, round or cut off. A diamond, a hexagon, a
/// circle or a stadium turns all along its sides: none of them is a box.
/// An open outline is closed, as a fill closes it.
fn bend_of(path: &usvg::Path) -> Option<Bend> {
    use usvg::tiny_skia_path::{PathSegment, Point};
    let ts = path.abs_transform();
    if ts.kx != 0.0 || ts.ky != 0.0 {
        return None;
    }
    let (sx, sy) = (ts.sx.abs(), ts.sy.abs());
    let bounds = path.data().bounds();
    let (width, height) = (bounds.width() * sx, bounds.height() * sy);
    let most = width.min(height) / 3.0;
    let mut sides = 0;
    let mut bend = Bend {
        radius: 0.0,
        inset: 0.0,
    };
    // A run from `a` to `b`: a side, or a corner, or neither.
    let mut run = |a: Point, b: Point| -> bool {
        let (dx, dy) = ((b.x - a.x).abs() * sx, (b.y - a.y).abs() * sy);
        let (long, short) = (dx.max(dy), dx.min(dy));
        if long < 1e-3 {
            return true;
        }
        // A corner's arc runs as far across as down.
        if short <= SKEW.max(long * SLANT) && short * 3.0 <= long {
            sides += 1;
            // How far in a side that runs along much of the box lies; a
            // short one only steps from one side to the next.
            let inset = if dx >= dy {
                let y = (a.y + b.y) / 2.0;
                (long >= width / 4.0).then(|| (y - bounds.top()).min(bounds.bottom() - y) * sy)
            } else {
                let x = (a.x + b.x) / 2.0;
                (long >= height / 4.0).then(|| (x - bounds.left()).min(bounds.right() - x) * sx)
            };
            bend.inset = bend.inset.max(inset.unwrap_or(0.0));
            true
        } else if long <= most {
            bend.radius = bend.radius.max(long);
            true
        } else {
            false
        }
    };
    let (mut at, mut start) = (Point::zero(), Point::zero());
    let mut open = false;
    for segment in path.data().segments() {
        let to = match segment {
            PathSegment::MoveTo(p) => {
                if open && !run(at, start) {
                    return None;
                }
                (at, start, open) = (p, p, false);
                continue;
            }
            PathSegment::LineTo(p) | PathSegment::QuadTo(_, p) | PathSegment::CubicTo(_, _, p) => p,
            PathSegment::Close => start,
        };
        if !run(at, to) {
            return None;
        }
        open = !matches!(segment, PathSegment::Close);
        at = to;
    }
    if open && !run(at, start) {
        return None;
    }
    (sides >= 2).then_some(bend)
}

/// How far a box's side may run off straight, in diagram units, or as a
/// share of its length: the hand-drawn look shakes its ends by up to 2.4.
const SKEW: f32 = 5.0;
const SLANT: f32 = 0.15;

/// The topmost fill under `(cx, cy)`, page background included.
fn ground_at(shapes: &[Shape], cx: f32, cy: f32) -> Option<[f32; 3]> {
    shapes
        .iter()
        .rev()
        .find(|shape| shape.fill.is_some() && holds(&shape.rect, cx, cy))
        .and_then(|shape| shape.fill)
}

fn holds(rect: &Rect, cx: f32, cy: f32) -> bool {
    cx >= rect.left() && cx <= rect.right() && cy >= rect.top() && cy <= rect.bottom()
}

fn center(rect: &Rect) -> (f32, f32) {
    (
        rect.left() + rect.width() / 2.0,
        rect.top() + rect.height() / 2.0,
    )
}

/// Whether `label` is the name of `shape`, not a line's words lying in it.
/// The name is the topmost text that belongs to the shape itself, and the
/// lines stacked on that name. A nested box keeps its own name.
fn names_the_shape(
    label: &Text,
    shape: &Rect,
    texts: &[Text],
    shapes: &[Shape],
    page: usvg::Size,
) -> bool {
    let direct: Vec<&Text> = texts
        .iter()
        .filter(|text| {
            around(shapes, &text.rect, page).is_some_and(|found| std::ptr::eq(found, shape))
        })
        .collect();
    let Some(top) = direct
        .iter()
        .min_by(|a, b| a.rect.top().total_cmp(&b.rect.top()))
    else {
        return false;
    };
    let mut bottom = top.rect.bottom();
    loop {
        let next = direct
            .iter()
            .filter(|text| text.rect.top() <= bottom + HEADER_GAP)
            .map(|text| text.rect.bottom())
            .fold(bottom, f32::max);
        if next <= bottom + 0.1 {
            break;
        }
        bottom = next;
    }
    label.rect.top() <= bottom + 0.5
}

/// The place of the `nth` label (from 0) whose text is exactly `text`:
/// `(x, y, width, height)` in diagram units. A label that names a box
/// takes that box. A shape that covers the page is a background and does
/// not count. Words on a line, including words inside a box they
/// do not name, keep the label's own rectangle, grown by a few units.
pub fn spot(tree: &Tree, text: &str, nth: usize) -> Option<(f32, f32, f32, f32)> {
    Some(match seat(tree, text, nth)? {
        Seat::Shape { x, y, w, h } => (x, y, w, h),
        Seat::Words { x, y, w, h, .. } => (x - PAD, y - PAD, w + PAD * 2.0, h + PAD * 2.0),
    })
}

/// The labels and the shapes of `group`. The circle before a class's name
/// and its letter (`C`, `I`), in a group [`omaspec_uml::SPOT`] names, are
/// a mark of its kind, no words of the source: a click on them is a click
/// on its box, and the labels are counted without them.
fn collect(group: &usvg::Group, texts: &mut Vec<Text>, shapes: &mut Vec<Shape>) {
    for node in group.children() {
        match node {
            Node::Group(g) if g.id().starts_with(omaspec_uml::SPOT) => {}
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
                        ink: ink_of(t),
                    });
                }
            }
            Node::Path(p) => push_shape(p, shapes),
            _ => {}
        }
    }
}

/// A visible filled path. Lines and outlines with no fill are no shape.
fn push_shape(path: &usvg::Path, shapes: &mut Vec<Shape>) {
    if !path.is_visible() || path.fill().is_none_or(|fill| fill.opacity().get() <= 0.0) {
        return;
    }
    if let Some(rect) = path.abs_bounding_box().to_non_zero_rect() {
        shapes.push(Shape {
            rect: rect.to_rect(),
            fill: path.fill().and_then(|fill| paint_rgb(fill.paint())),
            stroked: path
                .stroke()
                .is_some_and(|stroke| stroke.opacity().get() > 0.0),
        });
    }
}

/// The letters' fill. SVG's own default is black.
fn ink_of(text: &usvg::Text) -> [f32; 3] {
    for chunk in text.chunks() {
        for span in chunk.spans() {
            if let Some(fill) = span.fill()
                && let Some(rgb) = paint_rgb(fill.paint())
            {
                return rgb;
            }
        }
    }
    [0.0, 0.0, 0.0]
}

fn paint_rgb(paint: &usvg::Paint) -> Option<[f32; 3]> {
    let color = match paint {
        usvg::Paint::Color(color) => *color,
        usvg::Paint::LinearGradient(gradient) => gradient.stops().first()?.color(),
        usvg::Paint::RadialGradient(gradient) => gradient.stops().first()?.color(),
        usvg::Paint::Pattern(_) => return None,
    };
    Some([
        f32::from(color.red) / 255.0,
        f32::from(color.green) / 255.0,
        f32::from(color.blue) / 255.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export;

    /// A click on the circle before a class's name, or on its letter, is
    /// a click on the class: its name, counted as if no letter stood.
    #[test]
    fn a_click_on_a_class_s_circle_finds_the_class() {
        let svg = omaspec_uml::render("@startuml\nclass C\nclass Book\n@enduml\n").unwrap();
        let tree = export::parse(&svg).unwrap();
        let at = |attr: &str, from: &str| -> f32 {
            let start = from.find(&format!(" {attr}=\"")).unwrap() + attr.len() + 3;
            from[start..].split('"').next().unwrap().parse().unwrap()
        };
        let second = &svg[svg.find("id=\"spot-Book\"").unwrap()..];
        let label = label_at(&tree, at("cx", second), at("cy", second)).unwrap();
        assert_eq!((label.text.as_str(), label.nth), ("Book", 0));
        // The class named C is the first `C` there is.
        let first = &svg[svg.find("id=\"spot-C\"").unwrap()..];
        let label = label_at(&tree, at("cx", first) + 20.0, at("cy", first)).unwrap();
        assert_eq!((label.text.as_str(), label.nth), ("C", 0));
        assert_eq!(texts(&tree), ["C", "Book"]);
    }

    #[test]
    fn a_box_has_corners_and_other_shapes_have_none() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="600" height="200">
<rect x="10" y="10" width="80" height="40" rx="8" fill="#f1f1f1" stroke="#181818"/>
<rect x="100" y="10" width="80" height="40" fill="#f1f1f1"/>
<rect x="190" y="10" width="80" height="40" rx="20" fill="#f1f1f1"/>
<circle cx="300" cy="30" r="20" fill="#f1f1f1"/>
<polygon points="360,10 380,30 360,50 340,30" fill="#f1f1f1"/>
<path d="M10,110 C40,112 70,108 100,111 C102,130 99,140 101,160 C70,158 40,162 9,159 C11,140 8,130 10,110 Z" fill="#f1f1f1"/>
<path d="M200,110 L240,110 L240,130" fill="#f1f1f1"/>
<rect x="300" y="110" width="120" height="60" rx="5" fill="none" stroke="#181818"/>
<polygon points="566,140 566,110 450,110 450,170 570,170 570,140" fill="#f1f1f1"/>
</svg>"##;
        let tree = export::parse(svg).unwrap();
        let fills = fills_of([tree.root()]);
        let radius = |x, y, w, h| fills.bend(x, y, w, h).map(|bend| bend.radius);
        let rounded = radius(10.0, 10.0, 80.0, 40.0).unwrap();
        assert!((rounded - 8.0).abs() < 0.5, "{rounded}");
        assert_eq!(
            fills.bend(100.0, 10.0, 80.0, 40.0),
            Some(Bend {
                radius: 0.0,
                inset: 0.0
            })
        );
        // A stadium, a circle and a diamond bend or slant all along.
        assert_eq!(radius(190.0, 10.0, 80.0, 40.0), None);
        assert_eq!(radius(280.0, 10.0, 40.0, 40.0), None);
        assert_eq!(radius(340.0, 10.0, 40.0, 40.0), None);
        // The hand-drawn look's box: bowed sides, its ends shaken, so its
        // sides lie a little in.
        let hand = fills.bend(9.0, 108.0, 93.0, 54.0).unwrap();
        assert_eq!(hand.radius, 0.0);
        assert!(hand.inset > 0.0 && hand.inset < 5.0, "{hand:?}");
        // An open outline, closed by its fill along a slant: a triangle.
        assert_eq!(radius(200.0, 110.0, 40.0, 20.0), None);
        // A container drawn only as its outline is a box, with no fill.
        let outlined = radius(300.0, 110.0, 120.0, 60.0).unwrap();
        assert!((outlined - 5.0).abs() < 0.5, "{outlined}");
        // A box whose right side steps in by 4, as a folded package's
        // does, and one drawn 3 inside the box asked about.
        let step = fills.bend(450.0, 110.0, 120.0, 60.0).unwrap();
        assert!((step.inset - 4.0).abs() < 0.01, "{step:?}");
        let within = fills.bend(97.0, 7.0, 86.0, 46.0).unwrap();
        assert!((within.inset - 3.0).abs() < 0.01, "{within:?}");
        assert_eq!(fills.of(300.0, 110.0, 120.0, 60.0), None);
    }

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
        assert!(matches!(seat(&tree, "Edit", 0), Some(Seat::Shape { .. })));
        assert!(matches!(
            seat(&tree, "two lines", 0),
            Some(Seat::Words { .. })
        ));
    }

    #[test]
    fn a_label_inside_a_state_does_not_take_the_state() {
        // The siblings keep the picture from being only Playing.
        let svg = omaspec_uml::render(
            "@startuml\n\
             left to right direction\n\
             state One\n\
             state Two\n\
             state Three\n\
             state Four\n\
             state Playing {\n\
               [*] --> Buffering\n\
               Buffering --> Streaming : buffer full\n\
               Streaming --> Buffering : network slow\n\
             }\n\
             @enduml\n",
        )
        .unwrap();
        let tree = export::parse(&svg).unwrap();
        let playing = seat(&tree, "Playing", 0).unwrap();
        let buffering = seat(&tree, "Buffering", 0).unwrap();
        let Seat::Shape { w: pw, h: ph, .. } = playing else {
            panic!("Playing is a box, got {playing:?}");
        };
        let Seat::Shape { w: bw, h: bh, .. } = buffering else {
            panic!("Buffering is its own box, got {buffering:?}");
        };
        assert!(
            bw < pw && bh < ph,
            "Buffering {bw}×{bh} inside Playing {pw}×{ph}"
        );
        for name in ["buffer full", "network slow"] {
            let Seat::Words {
                w, h, ink, ground, ..
            } = seat(&tree, name, 0).unwrap()
            else {
                panic!("{name} took a box");
            };
            let ink_l = 0.2126 * ink[0] + 0.7152 * ink[1] + 0.0722 * ink[2];
            assert!(ink_l < 0.25, "{name} ink {ink:?}");
            let ground = ground.expect("the label sits in Playing");
            let ground_l = 0.2126 * ground[0] + 0.7152 * ground[1] + 0.0722 * ground[2];
            assert!(ground_l > 0.7, "{name} sits on {ground:?}, not the page");
            assert!(
                w < pw * 0.8 && h < ph * 0.5,
                "{name} {w}×{h} inside {pw}×{ph}"
            );
        }
    }

    #[test]
    fn a_note_over_a_lifeline_marks_the_note() {
        // The first line of the note is centered on the activation bar, the
        // way PlantUML draws `note over` across a lifeline. The bar is the
        // smaller shape, and it used to take the line.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300">
            <rect x="230" y="20" width="8" height="240" fill="#000" fill-opacity="0"/>
            <rect x="150" y="60" width="20" height="160" fill="#ffffff"/>
            <path d="M40,100 H360 V160 H40 Z" fill="#FEFFDD"/>
            <text x="80" y="122" font-family="sans-serif" font-size="13" textLength="160">Dies st ein Kommentar</text>
            <text x="80" y="142" font-family="sans-serif" font-size="13" textLength="220">ein längerer Kommentar mit vielen Zeilen</text>
        </svg>"##;
        let tree = export::parse(svg).unwrap();
        let mut texts = Vec::new();
        let mut shapes = Vec::new();
        collect(tree.root(), &mut texts, &mut shapes);
        let first = texts
            .iter()
            .find(|text| text.text == "Dies st ein Kommentar")
            .unwrap();
        let (cx, cy) = center(&first.rect);
        assert!(
            shapes.iter().any(|shape| {
                shape.rect.width() < 30.0
                    && shape.rect.height() > 100.0
                    && holds(&shape.rect, cx, cy)
            }),
            "the line's middle has to sit on the bar for this test to count"
        );
        let mut note = None;
        for line in [
            "Dies st ein Kommentar",
            "ein längerer Kommentar mit vielen Zeilen",
        ] {
            let Seat::Shape { x, y, w, h } = seat(&tree, line, 0).unwrap() else {
                panic!("{line} took no box");
            };
            assert!(w > 200.0 && h < 80.0, "{line} sat on {w}×{h}");
            let here = (
                x.round() as i32,
                y.round() as i32,
                w.round() as i32,
                h.round() as i32,
            );
            match note {
                None => note = Some(here),
                Some(first) => assert_eq!(here, first, "{line} marked another part"),
            }
        }
    }

    #[test]
    fn a_state_that_fills_most_of_the_page_is_still_its_box() {
        let source = include_str!("../../../examples/gallery/media-player.mmd");
        let svg = crate::mermaid::render(source).unwrap();
        let tree = export::parse(&svg).unwrap();
        let Seat::Shape { w, h, .. } = seat(&tree, "On", 0).unwrap() else {
            panic!("On was only its word");
        };
        assert!(w > 200.0 && h > 200.0, "On {w}×{h}");
        let Seat::Shape { w: ow, .. } = seat(&tree, "Off", 0).unwrap() else {
            panic!("Off was only its word");
        };
        assert!(ow < w, "Off {ow} inside On {w}");
        assert!(matches!(seat(&tree, "power", 0), Some(Seat::Words { .. })));
        assert!(matches!(
            seat(&tree, "buffer full", 0),
            Some(Seat::Words { .. })
        ));
    }

    #[test]
    fn the_words_on_a_line_are_not_a_box() {
        let svg = crate::mermaid::render("flowchart LR\n  A[Start] -->|yes| B[Done]\n").unwrap();
        let tree = export::parse(&svg).unwrap();
        let seat = seat(&tree, "yes", 0).unwrap();
        assert!(matches!(seat, Seat::Words { .. }), "yes took {seat:?}");
        assert!(matches!(
            super::seat(&tree, "Start", 0),
            Some(Seat::Shape { .. })
        ));
    }

    #[test]
    fn one_container_on_the_whole_page_is_still_a_box() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="300" height="300">
            <rect x="0" y="0" width="300" height="300" fill="#ffffff"/>
            <rect x="10" y="10" width="280" height="280" fill="#f8fafc" stroke="#94a3b8"/>
            <text x="120" y="30" font-family="sans-serif" font-size="14">Playing</text>
            <rect x="100" y="130" width="100" height="40" fill="#f8fafc" stroke="#94a3b8"/>
            <text x="115" y="155" font-family="sans-serif" font-size="14">Buffering</text>
        </svg>"##;
        let tree = export::parse(svg).unwrap();
        let Some(Seat::Shape { w, h, .. }) = seat(&tree, "Playing", 0) else {
            panic!("Playing lost its box");
        };
        assert!(w > 250.0 && h > 250.0, "Playing {w}×{h}");
        // Empty room inside the container is the container, not a child.
        assert_eq!(label_at(&tree, 40.0, 250.0).unwrap().text, "Playing");
        assert_eq!(label_at(&tree, 105.0, 135.0).unwrap().text, "Buffering");
        // Past the container's margin is the page.
        assert_eq!(label_at(&tree, 4.0, 4.0), None);
    }

    #[test]
    fn fills_read_the_shape_on_the_box() {
        let tree = export::parse(SVG).unwrap();
        let fills = fills_of([tree.root()]);
        let fill = fills.of(10.0, 10.0, 120.0, 60.0).expect("the box");
        assert!((fill[0] - 238.0 / 255.0).abs() < 0.02, "{fill:?}");
        // A patch of empty paper is not the page behind it.
        assert!(fills.of(180.0, 100.0, 20.0, 20.0).is_none());
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
