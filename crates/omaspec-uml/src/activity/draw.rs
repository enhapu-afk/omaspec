//! Draws a laid out activity diagram: moves each swimlane's part into its
//! column, leads the lines that cross from one swimlane to another, and
//! writes the SVG. Without swimlanes everything stays where the layout
//! put it.

use super::layout::{Layout, SDL_BRACE, SDL_LINE, SDL_POINT, room};
use super::skin::Skin;
use super::tile::{Edge, Pt, Shape};
use super::{ActionShape, Diagram};
use crate::Declined;
use crate::annotations::Annotations;
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{self, Block};

/// Space around the drawing.
const MARGIN: f32 = 10.0;
/// Between a swimlane's border and what it holds.
const LANE_PAD: f32 = 10.0;
/// Arrow heads.
const HEAD_LENGTH: f32 = 10.0;
const HEAD_HALF: f32 = 4.0;

pub fn draw(d: &Diagram) -> Result<String, Declined> {
    let skin = Skin::from_params(&d.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    let family = canvas.family();
    let annotations = Annotations::new(
        &d.frame,
        [
            skin.title_font,
            skin.header_font,
            skin.footer_font,
            skin.caption_font,
        ],
        family,
    )?;
    let layout = Layout {
        skin: &skin,
        family,
    };
    let tile = layout.flow(&d.flow, 0)?;
    let titles = d
        .lanes
        .iter()
        .map(|l| {
            text::block(&l.title.value, skin.lane_font, family)
                .map_err(|e| Declined::at(l.title.line, e))
        })
        .collect::<Result<Vec<Block>, Declined>>()?;

    // What each swimlane holds, left and right, as the layout placed it.
    let lanes = d.lanes.len().max(1);
    let mut extent = vec![(f32::INFINITY, f32::NEG_INFINITY); lanes];
    let mut grow = |lane: usize, lo: f32, hi: f32| {
        if let Some(e) = extent.get_mut(lane) {
            *e = (e.0.min(lo), e.1.max(hi));
        }
    };
    for shape in &tile.shapes {
        match shape {
            Shape::Bar { spans, .. } => {
                for &(lo, hi, lane) in spans {
                    grow(lane, lo, hi);
                }
            }
            _ => {
                let (l, _, r, _) = shape.bounds();
                grow(shape.lane().unwrap_or(0), l, r);
            }
        }
    }
    for edge in &tile.edges {
        for p in &edge.points {
            grow(p.lane, p.x, p.x);
        }
    }

    // The columns: each wide enough for what it holds and its title.
    let mut dx = vec![0.0; lanes];
    let mut columns = Vec::with_capacity(lanes);
    let mut x = 0.0;
    for (lane, &(lo, hi)) in extent.iter().enumerate() {
        let (lo, hi) = if lo > hi { (0.0, 0.0) } else { (lo, hi) };
        if d.lanes.is_empty() {
            columns.push((lo, hi));
            break;
        }
        let title = titles.get(lane).map_or(0.0, |t| t.width);
        let width = (hi - lo + 2.0 * LANE_PAD).max(title + 2.0 * LANE_PAD);
        dx[lane] = x + (width - (hi - lo)) / 2.0 - lo;
        columns.push((x, x + width));
        x += width;
    }

    let lanes_top = annotations.top();
    let title_height = titles.iter().map(|t| t.height).fold(0.0, f32::max);
    let content_top = if d.lanes.is_empty() {
        lanes_top
    } else {
        lanes_top + title_height + 10.0
    };
    let dy = content_top - tile.bounds.1;
    let bottom = tile.bounds.3 + dy + if d.lanes.is_empty() { 0.0 } else { 10.0 };

    // Swimlanes: fills, titles and the lines between them.
    if !d.lanes.is_empty() {
        let stroke = Stroke::new(skin.lane_border, skin.lane_thickness);
        for (lane, &(left, right)) in columns.iter().enumerate() {
            if let Some(color) = &d.lanes[lane].color {
                let fill = crate::color::parse(color)
                    .map_err(|e| Declined::at(d.lanes[lane].title.line, e))?;
                canvas.rect(
                    Layer::Background,
                    (left, lanes_top, right - left, bottom - lanes_top),
                    0.0,
                    fill,
                    None,
                );
            }
            canvas.text(
                Layer::Front,
                &titles[lane],
                ((left + right) / 2.0, lanes_top),
                Align::Center,
            );
            if lane == 0 {
                canvas.line(Layer::Lifelines, (left, lanes_top), (left, bottom), stroke);
            }
            canvas.line(
                Layer::Lifelines,
                (right, lanes_top),
                (right, bottom),
                stroke,
            );
        }
    }

    let painter = Painter {
        skin: &skin,
        dx: &dx,
        dy,
    };
    for edge in &tile.edges {
        painter.edge(&mut canvas, edge);
    }
    for shape in &tile.shapes {
        painter.shape(&mut canvas, shape);
    }
    annotations.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, skin.background))
}

/// Draws shapes and lines moved into their swimlanes.
struct Painter<'a> {
    skin: &'a Skin,
    /// How far each swimlane's part moves to the right.
    dx: &'a [f32],
    /// How far everything moves down.
    dy: f32,
}

impl Painter<'_> {
    fn at(&self, lane: usize, x: f32, y: f32) -> (f32, f32) {
        (x + self.dx.get(lane).copied().unwrap_or(0.0), y + self.dy)
    }

    fn point(&self, p: Pt) -> (f32, f32) {
        self.at(p.lane, p.x, p.y)
    }

    /// The points of a line, moved: at a diamond it takes the corner that
    /// faces the rest of the line, and where it went straight down between
    /// two swimlanes it turns across halfway.
    fn route(&self, edge: &Edge) -> Vec<(f32, f32)> {
        let mut pts: Vec<(f32, f32)> = edge.points.iter().map(|&p| self.point(p)).collect();
        let last = pts.len() - 1;
        for (end, neighbor) in [(0, 1.min(last)), (last, last.saturating_sub(1))] {
            if let Some((left, right)) = edge.sides[usize::from(end != 0)] {
                let (l, r) = (self.point(left), self.point(right));
                let other = pts[neighbor].0;
                if other < l.0 {
                    pts[end] = l;
                } else if other > r.0 {
                    pts[end] = r;
                }
            }
        }
        let mut out: Vec<(f32, f32)> = Vec::with_capacity(pts.len() + 2);
        for p in pts {
            if let Some(&a) = out.last()
                && (a.0 - p.0).abs() > 0.01
                && (a.1 - p.1).abs() > 0.01
            {
                let mid = (a.1 + p.1) / 2.0;
                out.push((a.0, mid));
                out.push((p.0, mid));
            }
            if out.last() != Some(&p) {
                out.push(p);
            }
        }
        out
    }

    fn edge(&self, canvas: &mut Canvas, edge: &Edge) {
        let points = self.route(edge);
        if points.len() < 2 {
            return;
        }
        canvas.polyline(Layer::Frames, &points, edge.stroke);
        if edge.head {
            let tip = points[points.len() - 1];
            let from = points[points.len() - 2];
            let (len, ux, uy) = {
                let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
                let len = (vx * vx + vy * vy).sqrt();
                (len, vx / len.max(0.001), vy / len.max(0.001))
            };
            if len < 0.01 {
                return;
            }
            let back = (tip.0 - ux * HEAD_LENGTH, tip.1 - uy * HEAD_LENGTH);
            let side = (-uy * HEAD_HALF, ux * HEAD_HALF);
            let solid = Stroke {
                dash: None,
                width: edge.stroke.width.min(1.5),
                ..edge.stroke
            };
            let mut head = vec![
                (back.0 + side.0, back.1 + side.1),
                tip,
                (back.0 - side.0, back.1 - side.1),
            ];
            if !self.skin.strict {
                head.push((tip.0 - ux * 6.0, tip.1 - uy * 6.0));
            }
            canvas.polygon(Layer::Front, &head, Some(edge.stroke.color), Some(solid));
        }
    }

    fn shape(&self, canvas: &mut Canvas, shape: &Shape) {
        let skin = self.skin;
        match shape {
            Shape::Box {
                rect: (x, y, w, h),
                text,
                fill,
                shape,
                lane,
            } => {
                let (x, y) = self.at(*lane, *x, *y);
                let look = &skin.activity;
                let stroke = Stroke::new(look.border, look.thickness);
                action_shape(canvas, *shape, (x, y, *w, *h), skin.round, *fill, stroke);
                canvas.text(
                    Layer::Front,
                    text,
                    (x + skin.padding + room(*shape).1, y + skin.padding),
                    Align::Left,
                );
            }
            Shape::Diamond {
                rect: (x, y, w, h),
                text,
                lane,
            } => {
                let (x, y) = self.at(*lane, *x, *y);
                let (w, h) = (*w, *h);
                let cy = y + h / 2.0;
                let points = if text.is_empty() || skin.test_in_diamond {
                    vec![(x, cy), (x + w / 2.0, y), (x + w, cy), (x + w / 2.0, y + h)]
                } else {
                    let p = 12.0;
                    vec![
                        (x, cy),
                        (x + p, y),
                        (x + w - p, y),
                        (x + w, cy),
                        (x + w - p, y + h),
                        (x + p, y + h),
                    ]
                };
                let look = &skin.diamond;
                canvas.polygon(
                    Layer::Front,
                    &points,
                    look.fill,
                    Some(Stroke::new(look.border, look.thickness)),
                );
                canvas.text(
                    Layer::Front,
                    text,
                    (x + w / 2.0, y + (h - text.height) / 2.0),
                    Align::Center,
                );
            }
            Shape::Start { center, lane } => {
                let c = self.at(*lane, center.0, center.1);
                canvas.ellipse(
                    Layer::Front,
                    c,
                    (10.0, 10.0),
                    Some(skin.start),
                    Some(Stroke::new(skin.start, 1.0)),
                );
            }
            Shape::Stop { center, lane } => {
                let c = self.at(*lane, center.0, center.1);
                canvas.ellipse(
                    Layer::Front,
                    c,
                    (11.0, 11.0),
                    None,
                    Some(Stroke::new(skin.end, 1.0)),
                );
                canvas.ellipse(
                    Layer::Front,
                    c,
                    (6.0, 6.0),
                    Some(skin.end),
                    Some(Stroke::new(skin.end, 1.0)),
                );
            }
            Shape::End { center, lane } => {
                let (cx, cy) = self.at(*lane, center.0, center.1);
                let stroke = Stroke::new(skin.end, 1.5);
                canvas.ellipse(Layer::Front, (cx, cy), (10.0, 10.0), None, Some(stroke));
                let d = 6.2;
                canvas.line(Layer::Front, (cx - d, cy - d), (cx + d, cy + d), stroke);
                canvas.line(Layer::Front, (cx - d, cy + d), (cx + d, cy - d), stroke);
            }
            Shape::Bar { y, height, spans } => {
                let xs: Vec<f32> = spans
                    .iter()
                    .flat_map(|&(lo, hi, lane)| {
                        [self.at(lane, lo, 0.0).0, self.at(lane, hi, 0.0).0]
                    })
                    .collect();
                let lo = xs.iter().copied().fold(f32::INFINITY, f32::min);
                let hi = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
                if lo > hi {
                    return;
                }
                let y = y + self.dy;
                if *height > 0.0 {
                    canvas.rect(
                        Layer::Front,
                        (lo, y, hi - lo, *height),
                        2.0,
                        Some(skin.bar),
                        None,
                    );
                } else {
                    canvas.line(Layer::Frames, (lo, y), (hi, y), skin.arrow);
                }
            }
            Shape::Note {
                rect: (x, y, w, h),
                text,
                fill,
                lane,
            } => {
                let (x, y) = self.at(*lane, *x, *y);
                note(canvas, skin, (x, y, *w, *h), text, *fill);
            }
            Shape::Label {
                at,
                text,
                align,
                lane,
            } => {
                let at = self.at(*lane, at.0, at.1);
                canvas.text(Layer::Front, text, at, *align);
            }
            Shape::Frame {
                rect: (x, y, w, h),
                title,
                fill,
                lane,
            } => {
                let (x, y) = self.at(*lane, *x, *y);
                let look = &skin.partition;
                let stroke = Stroke::new(look.border, look.thickness);
                canvas.rect(Layer::Background, (x, y, *w, *h), 0.0, *fill, Some(stroke));
                let (tab_w, tab_h, corner) = if title.width == 0.0 {
                    (w / 3.0, 12.0, 7.0)
                } else {
                    (title.width + 10.0, title.height + 3.0, 10.0)
                };
                let d = format!(
                    "M{},{} L{},{} L{},{} L{},{}",
                    n(x + tab_w),
                    n(y),
                    n(x + tab_w),
                    n(y + tab_h - corner),
                    n(x + tab_w - corner),
                    n(y + tab_h),
                    n(x),
                    n(y + tab_h)
                );
                canvas.path(
                    Layer::Background,
                    &d,
                    (x, y, x + tab_w, y + tab_h),
                    None,
                    Some(stroke),
                );
                canvas.text(Layer::Background, title, (x + 3.0, y + 1.0), Align::Left);
            }
        }
    }
}

/// A note: a sheet with a folded corner.
fn note(
    canvas: &mut Canvas,
    skin: &Skin,
    (x, y, w, h): (f32, f32, f32, f32),
    text: &Block,
    fill: crate::color::Paint,
) {
    let look = &skin.note;
    let stroke = Some(Stroke::new(look.border, look.thickness));
    let fold = 10.0;
    let d = format!(
        "M{},{} L{},{} L{},{} L{},{} L{},{} Z",
        n(x),
        n(y),
        n(x),
        n(y + h),
        n(x + w),
        n(y + h),
        n(x + w),
        n(y + fold),
        n(x + w - fold),
        n(y)
    );
    canvas.path(Layer::Front, &d, (x, y, x + w, y + h), fill, stroke);
    let corner = format!(
        "M{},{} L{},{} L{},{}",
        n(x + w - fold),
        n(y),
        n(x + w - fold),
        n(y + fold),
        n(x + w),
        n(y + fold)
    );
    canvas.path(Layer::Front, &corner, (x, y, x + w, y + h), fill, stroke);
    canvas.text(Layer::Front, text, (x + 6.0, y + 5.0), Align::Left);
}

/// The outline of an action of `shape` in `(x, y, w, h)`.
fn action_shape(
    canvas: &mut Canvas,
    shape: ActionShape,
    (x, y, w, h): (f32, f32, f32, f32),
    round: f32,
    fill: crate::color::Paint,
    stroke: Stroke,
) {
    let (r, b, cy) = (x + w, y + h, y + h / 2.0);
    let p = SDL_POINT;
    let polygon = |canvas: &mut Canvas, points: &[(f32, f32)]| {
        canvas.polygon(Layer::Front, points, fill, Some(stroke));
    };
    match shape {
        ActionShape::Plain => canvas.rect(Layer::Front, (x, y, w, h), round, fill, Some(stroke)),
        ActionShape::Task | ActionShape::Object => {
            canvas.rect(Layer::Front, (x, y, w, h), 0.0, fill, Some(stroke));
        }
        ActionShape::Procedure => {
            canvas.rect(Layer::Front, (x, y, w, h), 0.0, fill, Some(stroke));
            canvas.line(Layer::Front, (x + SDL_LINE, y), (x + SDL_LINE, b), stroke);
            canvas.line(Layer::Front, (r - SDL_LINE, y), (r - SDL_LINE, b), stroke);
        }
        ActionShape::Input => polygon(canvas, &[(x, y), (r, y), (r - p, cy), (r, b), (x, b)]),
        ActionShape::Output | ActionShape::SendSignal => {
            polygon(canvas, &[(x, y), (r - p, y), (r, cy), (r - p, b), (x, b)]);
        }
        ActionShape::AcceptEvent => polygon(canvas, &[(x, y), (r, y), (r, b), (x, b), (x + p, cy)]),
        ActionShape::Save => polygon(canvas, &[(x + p, y), (r, y), (r - p, b), (x, b)]),
        ActionShape::Load => polygon(canvas, &[(x, y), (r - p, y), (r, b), (x + p, b)]),
        ActionShape::Continuous => {
            let d = SDL_BRACE;
            let path = format!(
                "M{},{} L{},{} C{},{} {},{} {},{} C{},{} {},{} {},{} L{},{} C{},{} {},{} {},{} C{},{} {},{} {},{} Z",
                n(x + d),
                n(y),
                n(r - d),
                n(y),
                n(r - d / 2.0),
                n(y),
                n(r - d / 2.0),
                n(cy),
                n(r),
                n(cy),
                n(r - d / 2.0),
                n(cy),
                n(r - d / 2.0),
                n(b),
                n(r - d),
                n(b),
                n(x + d),
                n(b),
                n(x + d / 2.0),
                n(b),
                n(x + d / 2.0),
                n(cy),
                n(x),
                n(cy),
                n(x + d / 2.0),
                n(cy),
                n(x + d / 2.0),
                n(y),
                n(x + d),
                n(y),
            );
            canvas.path(Layer::Front, &path, (x, y, r, b), fill, Some(stroke));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::render;

    #[test]
    fn the_deepest_nesting_draws_on_a_thread_of_its_own() {
        // As the editor's render thread runs it: a spawned thread's stack.
        let source = format!(
            "start\n{}:core;\n{}stop",
            "while (x)\nfork\n:a;\nfork again\n".repeat(50),
            "end fork\nendwhile\n".repeat(50)
        );
        let svg = std::thread::spawn(move || render(&source))
            .join()
            .expect("no stack overflow")
            .expect("draws");
        assert!(svg.contains(">core</tspan>"));
    }

    #[test]
    fn swimlanes_get_a_column_each() {
        let svg = render("|A|\nstart\n:one;\n|B|\n:two;\n|A|\nstop").unwrap();
        // The borders of two lanes, the arrow from one to the other turns.
        assert_eq!(svg.matches("<line ").count(), 3);
        assert!(svg.contains(">A</tspan>") && svg.contains(">B</tspan>"));
        let without = render("start\n:one;\n:two;\nstop").unwrap();
        assert_eq!(without.matches("<line ").count(), 0);
    }
}
