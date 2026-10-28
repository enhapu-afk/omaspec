//! What each box is, how big it is around its text, and its outline.
//!
//! The layout places bounding boxes and ends lines on their sides; a line
//! that meets a box whose outline lies inside its bounds (a circle, a
//! diamond, a slanted side) is carried on to the outline before it is
//! drawn ([`clip`]).

use mermaid_rs_renderer::NodeShape;
use omagram_uml::graph::Rect;

/// A box of a state diagram or a flowchart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    /// A state: a rounded box with a bold title and its description.
    State,
    /// A state choice: a diamond.
    Choice,
    Bar,
    Start,
    End,
    /// One concurrent region of a state: no box and no name.
    Region,
    /// A flowchart subgraph: a frame with its title on top.
    Cluster,
    /// A flowchart node.
    Node(Shape),
}

/// The shapes of a flowchart node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    /// `[text]`
    Rect,
    /// `(text)`
    Rounded,
    /// `([text])`
    Stadium,
    /// `[[text]]`
    Subroutine,
    /// `[(text)]`
    Cylinder,
    /// `((text))`
    Circle,
    /// `(((text)))`
    DoubleCircle,
    /// `{text}`
    Diamond,
    /// `{{text}}`
    Hexagon,
    /// `[/text/]`
    LeanRight,
    /// `[\text\]`
    LeanLeft,
    /// `[/text\]`
    Trapezoid,
    /// `[\text/]`
    TrapezoidAlt,
    /// `>text]`
    Flag,
    /// An icon with no `form`: the short name, and no box behind it.
    Label,
}

impl Kind {
    /// The kind of a state diagram's node, if this draws it.
    pub(super) fn of_state(shape: NodeShape) -> Option<Kind> {
        Some(match shape {
            NodeShape::RoundRect => Kind::State,
            NodeShape::Circle => Kind::Start,
            NodeShape::DoubleCircle => Kind::End,
            NodeShape::Diamond => Kind::Choice,
            NodeShape::ForkJoin => Kind::Bar,
            _ => return None,
        })
    }

    /// Whether a line may meet the box anywhere along a side, not only in
    /// the middle.
    pub(super) fn flat(self) -> bool {
        match self {
            Kind::State | Kind::Bar | Kind::Region | Kind::Cluster => true,
            Kind::Node(shape) => {
                matches!(
                    shape,
                    Shape::Rect | Shape::Rounded | Shape::Subroutine | Shape::Label
                )
            }
            Kind::Choice | Kind::Start | Kind::End => false,
        }
    }

    /// The outline lines end on, when it lies inside the box `rect`.
    pub(super) fn outline(self, rect: Rect) -> Option<Vec<(f32, f32)>> {
        match self {
            Kind::Choice => Some(Shape::Diamond.outline(rect)),
            Kind::Start | Kind::End => Some(Shape::Circle.outline(rect)),
            Kind::Node(shape) if !self.flat() => Some(shape.outline(rect)),
            _ => None,
        }
    }
}

impl Shape {
    /// mmdr reads `((text))` and `(((text)))` both as a double circle;
    /// `double` tells which one was written.
    pub(super) fn of(shape: NodeShape, double: bool) -> Option<Shape> {
        Some(match shape {
            NodeShape::Rectangle => Shape::Rect,
            NodeShape::RoundRect => Shape::Rounded,
            NodeShape::Stadium => Shape::Stadium,
            NodeShape::Subroutine => Shape::Subroutine,
            NodeShape::Cylinder => Shape::Cylinder,
            NodeShape::Circle => Shape::Circle,
            NodeShape::DoubleCircle if double => Shape::DoubleCircle,
            NodeShape::DoubleCircle => Shape::Circle,
            NodeShape::Diamond => Shape::Diamond,
            NodeShape::Hexagon => Shape::Hexagon,
            NodeShape::Parallelogram => Shape::LeanRight,
            NodeShape::ParallelogramAlt => Shape::LeanLeft,
            NodeShape::Trapezoid => Shape::Trapezoid,
            NodeShape::TrapezoidAlt => Shape::TrapezoidAlt,
            NodeShape::Asymmetric => Shape::Flag,
            _ => return None,
        })
    }

    /// The box around text of `width` and `height`.
    pub(super) fn size(self, width: f32, height: f32) -> (f32, f32) {
        let w = (width + PAD_X * 2.0).max(MIN_W);
        let h = (height + PAD_Y * 2.0).max(MIN_H);
        match self {
            Shape::Rect | Shape::Rounded | Shape::Label => (w, h),
            Shape::Stadium => ((w + h / 2.0).max(h), h),
            Shape::Subroutine => (w + 2.0 * SUBROUTINE, h),
            Shape::Cylinder => (w, h + 2.0 * cap(w)),
            Shape::Circle => {
                let d = circle(width, height);
                (d, d)
            }
            Shape::DoubleCircle => {
                let d = circle(width, height) + 2.0 * RING;
                (d, d)
            }
            Shape::Diamond => {
                // Any `t` keeps the text's corners on the outline; a wide
                // label gets a wider, flatter diamond than Mermaid's
                // square one.
                let (w, h) = (width + 16.0, height + 12.0);
                let t = (w / (h * 2.5)).clamp(1.0, 3.0);
                ((w + h * t).max(MIN_H), (h + w / t).max(MIN_H))
            }
            Shape::Hexagon => (w + h / 2.0, h),
            Shape::LeanRight | Shape::LeanLeft => (w + slant(h), h),
            Shape::Trapezoid | Shape::TrapezoidAlt => (w + 2.0 * slant(h), h),
            Shape::Flag => (w + slant(h), h),
        }
    }

    /// The middle of where the text goes in `rect`.
    pub(super) fn text_center(self, rect: Rect) -> (f32, f32) {
        let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        match self {
            Shape::Cylinder => (cx, cy + cap(rect.w)),
            Shape::Flag => (cx + slant(rect.h) / 2.0, cy),
            _ => (cx, cy),
        }
    }

    /// The outline as a polygon, the curves of a circle, stadium or
    /// cylinder in short steps.
    pub(super) fn outline(self, r: Rect) -> Vec<(f32, f32)> {
        let (x, y, w, h) = (r.x, r.y, r.w, r.h);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        match self {
            Shape::Rect | Shape::Rounded | Shape::Subroutine | Shape::Label => {
                vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
            }
            Shape::Stadium => {
                let radius = h / 2.0;
                let mut points = arc((x + w - radius, cy), (radius, radius), -90.0, 90.0);
                points.extend(arc((x + radius, cy), (radius, radius), 90.0, 270.0));
                points
            }
            Shape::Cylinder => {
                let ry = cap(w);
                let mut points = arc((cx, y + ry), (w / 2.0, ry), 180.0, 360.0);
                points.extend(arc((cx, y + h - ry), (w / 2.0, ry), 0.0, 180.0));
                points
            }
            Shape::Circle | Shape::DoubleCircle => arc((cx, cy), (w / 2.0, h / 2.0), 0.0, 360.0),
            Shape::Diamond => vec![(cx, y), (x + w, cy), (cx, y + h), (x, cy)],
            Shape::Hexagon => {
                let m = h / 4.0;
                vec![
                    (x + m, y),
                    (x + w - m, y),
                    (x + w, cy),
                    (x + w - m, y + h),
                    (x + m, y + h),
                    (x, cy),
                ]
            }
            Shape::LeanRight => {
                let s = slant(h);
                vec![(x + s, y), (x + w, y), (x + w - s, y + h), (x, y + h)]
            }
            Shape::LeanLeft => {
                let s = slant(h);
                vec![(x, y), (x + w - s, y), (x + w, y + h), (x + s, y + h)]
            }
            Shape::Trapezoid => {
                let s = slant(h);
                vec![(x + s, y), (x + w - s, y), (x + w, y + h), (x, y + h)]
            }
            Shape::TrapezoidAlt => {
                let s = slant(h);
                vec![(x, y), (x + w, y), (x + w - s, y + h), (x + s, y + h)]
            }
            Shape::Flag => {
                let s = slant(h);
                vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h), (x + s, cy)]
            }
        }
    }
}

/// Room around a flowchart node's text.
const PAD_X: f32 = 16.0;
const PAD_Y: f32 = 10.0;
const MIN_W: f32 = 40.0;
const MIN_H: f32 = 36.0;
/// From a subroutine's outer side to its inner line.
pub(super) const SUBROUTINE: f32 = 8.0;
/// Between the two rings of a double circle.
pub(super) const RING: f32 = 5.0;

/// The height of a cylinder's top cap below its edge, for a cylinder
/// `width` wide (Mermaid's proportion).
pub(super) fn cap(width: f32) -> f32 {
    width / 2.0 / (2.5 + width / 50.0)
}

/// How far a slanted side leans over a box `height` tall.
fn slant(height: f32) -> f32 {
    height * 0.35
}

/// A circle that holds text of `width` and `height` with a margin.
fn circle(width: f32, height: f32) -> f32 {
    ((width * width + height * height).sqrt() + 12.0).max(MIN_H)
}

/// Points along an ellipse from `start` to `end` degrees, clockwise on
/// screen (0 is to the right, 90 below).
fn arc(center: (f32, f32), radius: (f32, f32), start: f32, end: f32) -> Vec<(f32, f32)> {
    // A step of 7.5 degrees: a circle 120 across strays half a unit.
    let steps = ((end - start).abs() / 7.5).ceil().max(4.0) as usize;
    (0..=steps)
        .map(|step| {
            let angle = (start + (end - start) * step as f32 / steps as f32).to_radians();
            (
                center.0 + radius.0 * angle.cos(),
                center.1 + radius.1 * angle.sin(),
            )
        })
        .collect()
}

/// `points` with its end carried on from the side of `rect` to where the
/// line meets `outline`, so it touches the shape. Lines that already end
/// on the outline, or do not reach it, stay as they are.
pub(super) fn clip(points: &mut [(f32, f32)], outline: &[(f32, f32)], at_start: bool) {
    let len = points.len();
    if len < 2 || outline.len() < 3 {
        return;
    }
    let (end, before) = if at_start { (0, 1) } else { (len - 1, len - 2) };
    let (tip, from) = (points[end], points[before]);
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 0.01 {
        return;
    }
    let (ux, uy) = (dx / length, dy / length);
    let mut best: Option<f32> = None;
    for index in 0..outline.len() {
        let a = outline[index];
        let b = outline[(index + 1) % outline.len()];
        if let Some(t) = ray_hit(tip, (ux, uy), a, b)
            && t >= -0.01
            && best.is_none_or(|old| t < old)
        {
            best = Some(t);
        }
    }
    if let Some(t) = best.filter(|t| *t > 0.01) {
        points[end] = (tip.0 + ux * t, tip.1 + uy * t);
    }
}

/// How far along the ray from `origin` in direction `dir` it meets the
/// segment `a`–`b`.
fn ray_hit(origin: (f32, f32), dir: (f32, f32), a: (f32, f32), b: (f32, f32)) -> Option<f32> {
    let (ex, ey) = (b.0 - a.0, b.1 - a.1);
    let denom = dir.0 * ey - dir.1 * ex;
    if denom.abs() < 1e-6 {
        return None;
    }
    let (ox, oy) = (a.0 - origin.0, a.1 - origin.1);
    let t = (ox * ey - oy * ex) / denom;
    let s = (ox * dir.1 - oy * dir.0) / denom;
    (-0.001..=1.001).contains(&s).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 60.0,
        }
    }

    #[test]
    fn a_line_into_a_diamond_reaches_its_side() {
        let outline = Shape::Diamond.outline(rect());
        // Down onto the top, left of the tip.
        let mut points = vec![(25.0, -40.0), (25.0, 0.0)];
        clip(&mut points, &outline, false);
        assert!((points[1].1 - 15.0).abs() < 0.1, "{points:?}");
        // The tip itself stays.
        let mut tip = vec![(50.0, -40.0), (50.0, 0.0)];
        clip(&mut tip, &outline, false);
        assert!((tip[1].1).abs() < 0.1, "{tip:?}");
    }

    #[test]
    fn a_line_out_of_a_circle_starts_on_it() {
        let outline = Shape::Circle.outline(Rect { w: 60.0, ..rect() });
        let mut points = vec![(60.0, 10.0), (100.0, 10.0)];
        clip(&mut points, &outline, true);
        assert!(points[0].0 < 60.0 && points[0].0 > 50.0, "{points:?}");
    }

    #[test]
    fn every_shape_holds_its_text() {
        for shape in [
            Shape::Rect,
            Shape::Stadium,
            Shape::Cylinder,
            Shape::Circle,
            Shape::DoubleCircle,
            Shape::Diamond,
            Shape::Hexagon,
            Shape::LeanRight,
            Shape::Trapezoid,
            Shape::Flag,
            Shape::Label,
        ] {
            let (tw, th) = (120.0, 20.0);
            let (w, h) = shape.size(tw, th);
            let rect = Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            };
            let (cx, cy) = shape.text_center(rect);
            let outline = shape.outline(rect);
            for corner in [
                (cx - tw / 2.0, cy - th / 2.0),
                (cx + tw / 2.0, cy - th / 2.0),
                (cx + tw / 2.0, cy + th / 2.0),
                (cx - tw / 2.0, cy + th / 2.0),
            ] {
                assert!(inside(&outline, corner), "{shape:?} {corner:?} {outline:?}");
            }
        }
    }

    fn inside(polygon: &[(f32, f32)], (px, py): (f32, f32)) -> bool {
        let mut hit = false;
        for index in 0..polygon.len() {
            let (ax, ay) = polygon[index];
            let (bx, by) = polygon[(index + 1) % polygon.len()];
            if (ay > py) != (by > py) && px < ax + (py - ay) * (bx - ax) / (by - ay) + 0.5 {
                hit = !hit;
            }
        }
        hit
    }
}
