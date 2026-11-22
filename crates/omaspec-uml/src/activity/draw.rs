//! Draws a laid out activity diagram: moves each swimlane's part into its
//! column, leads the lines that cross from one swimlane to another, and
//! writes the SVG. Without swimlanes everything stays where the layout
//! put it.

use super::layout::{Layout, PACKAGE_SLANT, SDL_BRACE, SDL_LINE, SDL_POINT, room};
use super::skin::{Condition, Skin};
use super::tile::{Edge, Pt, Shape};
use super::{ActionShape, Diagram, FrameKind};
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
    draw_mapped(d).map(|(svg, _)| svg)
}

/// [`draw`], and the map: the swimlanes' titles across the top, which a
/// pane keeps in sight as it scrolls past them ([`crate::map::Map::bands`],
/// REQ-073).
pub fn draw_mapped(d: &Diagram) -> Result<(String, crate::map::Map), Declined> {
    let mut skin = Skin::from_params(&d.frame.skinparams)?;
    skin.vertical_if = d.frame.vertical_if;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    canvas.sketch(skin.sketch);
    canvas.shadows(skin.shadow);
    if d.frame.parts {
        canvas.mark_parts();
    }
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
                canvas.plain(|canvas| {
                    canvas.rect(
                        Layer::Background,
                        (left, lanes_top, right - left, bottom - lanes_top),
                        0.0,
                        fill,
                        None,
                    );
                });
            }
            // A swimlane's name and each line between two are parts no
            // box takes: they glide on their own (docs/adr/0029).
            canvas.part(&format!("lane:{lane}"), |canvas| {
                canvas.text(
                    Layer::Front,
                    &titles[lane],
                    ((left + right) / 2.0, lanes_top),
                    Align::Center,
                );
            });
            if lane == 0 {
                canvas.part("border:left", |canvas| {
                    canvas.line(Layer::Lifelines, (left, lanes_top), (left, bottom), stroke);
                });
            }
            canvas.part(&format!("border:{lane}"), |canvas| {
                canvas.line(
                    Layer::Lifelines,
                    (right, lanes_top),
                    (right, bottom),
                    stroke,
                );
            });
        }
    }

    let painter = Painter {
        skin: &skin,
        dx: &dx,
        dy,
    };
    // Each shape in its part, and each line told to the map from the shape
    // it leaves to the one it enters (docs/adr/0029).
    let mut found = super::parts::Found::default();
    for shape in &tile.shapes {
        let part = found.shape(shape, painter.rect(shape));
        canvas.part(&part, |canvas| painter.shape(canvas, shape));
    }
    for edge in &tile.edges {
        if let Some((path, points, head)) = painter.edge(&mut canvas, edge) {
            found.run(path, points, head, edge.stroke);
        }
    }
    annotations.draw(&mut canvas);
    let titles = match (columns.first(), columns.last()) {
        (Some(&(left, _)), Some(&(_, right))) if !d.lanes.is_empty() => vec![crate::map::Band {
            rect: crate::graph::Rect {
                x: left,
                y: lanes_top,
                w: right - left,
                h: content_top - lanes_top,
            },
            edge: crate::map::Edge::Top,
            // The lanes under their names, to their foot.
            over: crate::graph::Rect {
                x: left,
                y: content_top,
                w: right - left,
                h: bottom - content_top,
            },
        }],
        _ => Vec::new(),
    };
    let map = canvas.place(
        crate::map::Map {
            bands: titles,
            // Only a pane that glides asks for the parts, and only it reads
            // the lines.
            wires: if d.frame.parts {
                found.wires()
            } else {
                Vec::new()
            },
            places: found.places,
            ..crate::map::Map::default()
        },
        MARGIN,
    );
    Ok((canvas.finish(MARGIN, skin.background), map))
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

    /// Where `shape` is drawn: its box, moved into its swimlane.
    fn rect(&self, shape: &Shape) -> crate::graph::Rect {
        let (left, top, right, bottom) = shape.bounds();
        let (left, right) = match shape {
            Shape::Bar { spans, .. } => spans
                .iter()
                .flat_map(|&(lo, hi, lane)| [self.at(lane, lo, 0.0).0, self.at(lane, hi, 0.0).0])
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), x| {
                    (lo.min(x), hi.max(x))
                }),
            _ => {
                let dx = self.at(shape.lane().unwrap_or(0), 0.0, 0.0).0;
                (left + dx, right + dx)
            }
        };
        crate::graph::Rect {
            x: left,
            y: top + self.dy,
            w: (right - left).max(0.0),
            h: bottom - top,
        }
    }

    /// Draws `edge`; its corners, its points as drawn (the corners
    /// rounded), and those of its head.
    #[allow(clippy::type_complexity)]
    fn edge(
        &self,
        canvas: &mut Canvas,
        edge: &Edge,
    ) -> Option<(Vec<(f32, f32)>, Vec<(f32, f32)>, Option<Vec<(f32, f32)>>)> {
        let points = self.route(edge);
        // A hidden arrow is neither drawn nor a line of the map.
        if points.len() < 2 || edge.stroke.width <= 0.0 {
            return None;
        }
        // The head sits on a straight end, not on the curve before it;
        // `ArrowHeadColor none` draws none, but on an arrow written in a
        // style of its own (`-[#red]->`, `-[dashed]->`), as PlantUML.
        let headed = edge.head && (self.skin.heads || edge.styled);
        let arrive = if headed { HEAD_LENGTH } else { 0.0 };
        canvas.curve_arriving(Layer::Lines, &points, self.skin.curve, arrive, edge.stroke);
        let curved = crate::curve::flatten_arriving(&points, self.skin.curve, arrive);
        let mut drawn = None;
        if headed {
            let tip = points[points.len() - 1];
            let from = points[points.len() - 2];
            let (len, ux, uy) = {
                let (vx, vy) = (tip.0 - from.0, tip.1 - from.1);
                let len = (vx * vx + vy * vy).sqrt();
                (len, vx / len.max(0.001), vy / len.max(0.001))
            };
            if len < 0.01 {
                return Some((points, curved, None));
            }
            let back = (tip.0 - ux * HEAD_LENGTH, tip.1 - uy * HEAD_LENGTH);
            let side = (-uy * HEAD_HALF, ux * HEAD_HALF);
            let color = edge.stroke.color;
            let solid = Stroke {
                dash: None,
                width: edge.stroke.width.min(1.5),
                color,
            };
            let mut head = vec![
                (back.0 + side.0, back.1 + side.1),
                tip,
                (back.0 - side.0, back.1 - side.1),
            ];
            if !self.skin.strict {
                head.push((tip.0 - ux * 6.0, tip.1 - uy * 6.0));
            }
            canvas.polygon(Layer::Lines, &head, Some(color), Some(solid));
            drawn = Some(head);
        }
        Some((points, curved, drawn))
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
                let points = if text.is_empty() || skin.condition == Condition::InsideDiamond {
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
            Shape::Connector {
                center,
                radius,
                text,
                fill,
                lane,
            } => {
                let c = self.at(*lane, center.0, center.1);
                let look = &skin.activity;
                let stroke = Stroke::new(look.border, look.thickness);
                canvas.ellipse(Layer::Front, c, (*radius, *radius), *fill, Some(stroke));
                canvas.text(
                    Layer::Front,
                    text,
                    (c.0, c.1 - text.height / 2.0),
                    Align::Center,
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
                    canvas.card(Layer::Front, (lo, y, hi - lo, *height), 2.0, skin.bar);
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
            Shape::Test { at, text, lane } => {
                let at = self.at(*lane, at.0, at.1);
                canvas.text(Layer::Front, text, at, Align::Left);
            }
            Shape::Frame {
                rect: (x, y, w, h),
                title,
                title_at,
                fill,
                kind,
                lane,
            } => {
                let (x, y) = self.at(*lane, *x, *y);
                let (w, h) = (*w, *h);
                let look = &skin.partition;
                let stroke = Stroke::new(look.border, look.thickness);
                match kind {
                    FrameKind::Tab => {}
                    FrameKind::Package => {
                        // A folder: the tab above the frame, slanting down.
                        let (tab_w, tab_h) = (title.width + 8.0, title.height + 6.0);
                        let corner = x + tab_w + PACKAGE_SLANT;
                        let outline = [
                            (x, y),
                            (x + tab_w, y),
                            (corner, y + tab_h),
                            (x + w, y + tab_h),
                            (x + w, y + h),
                            (x, y + h),
                        ];
                        canvas.polygon(Layer::Background, &outline, *fill, Some(stroke));
                        canvas.line(
                            Layer::Background,
                            (x, y + tab_h),
                            (corner, y + tab_h),
                            stroke,
                        );
                        canvas.text(
                            Layer::Background,
                            title,
                            (x + title_at, y + 3.0),
                            Align::Left,
                        );
                        return;
                    }
                    FrameKind::Rectangle | FrameKind::Card => {
                        let round = skin.partition_round;
                        canvas.rect(Layer::Background, (x, y, w, h), round, *fill, Some(stroke));
                        if *kind == FrameKind::Card {
                            let band = y + title.height + 6.0;
                            canvas.line(Layer::Background, (x, band), (x + w, band), stroke);
                        }
                        canvas.text(
                            Layer::Background,
                            title,
                            (x + title_at, y + 3.0),
                            Align::Left,
                        );
                        return;
                    }
                }
                let round = skin.partition_round;
                canvas.rect(Layer::Background, (x, y, w, h), round, *fill, Some(stroke));
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
    /// A fork's bar spans a flow that ends before it, so the line on from
    /// its middle starts on it.
    #[test]
    fn a_forks_bar_spans_a_flow_that_ends() {
        let source = "@startuml\nstart\nfork\n  :B;\nfork again\n  :d;\n  detach\nend fork\n:after;\n@enduml\n";
        let (svg, _) = crate::render_mapped(source, None).unwrap();
        // The bar that joins the flows is as wide as the one that splits
        // them.
        let bars = bars(&svg);
        assert_eq!(bars.len(), 2, "{svg}");
        assert_eq!((bars[0].0, bars[0].2), (bars[1].0, bars[1].2), "{svg}");
    }

    /// A `<style>` reaches a partition by PlantUML's name for it, its
    /// corners too; what the diagram's own element sets reaches it first.
    #[test]
    fn a_style_reaches_a_partition() {
        let source = "@startuml\n<style>\nactivityDiagram {\n  BackgroundColor #33668E\n  FontName Arial\n  partition {\n    LineColor red\n    RoundCorner 10\n    BackgroundColor PeachPuff\n  }\n}\n</style>\nstart\npartition End {\n  :end;\n}\nstop\n@enduml\n";
        let svg = crate::render(source).unwrap();
        let frame = svg
            .split("<rect")
            .find(|r| r.contains("fill=\"#FFDAB9\""))
            .unwrap_or_else(|| panic!("{svg}"));
        assert!(
            frame.contains("stroke=\"#FF0000\"") && frame.contains("rx=\"5\""),
            "{frame}"
        );
    }

    /// The name of a `rectangle` or a `card` stands in the middle at the
    /// top, but gives way to the line into its flow.
    #[test]
    fn a_frames_name_gives_way_to_the_line_in() {
        for kind in ["rectangle", "card"] {
            let source = format!(
                "@startuml\nstart\n{kind} \"A rather long name\" {{\n:A;\n}}\nstop\n@enduml\n"
            );
            let (_, map) = crate::render_mapped(&source, None).unwrap();
            let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
            let (name, action) = (at(2), at(3));
            let line = action.x + action.w / 2.0;
            assert!(
                name.x + name.w + 4.0 <= line || name.x >= line + 4.0,
                "{kind}: {name:?} {line}"
            );
        }
    }

    /// `!pragma useVerticalIf on` stacks the tests of an `if` with
    /// `elseif`s, each below the branch before, each branch right of its
    /// test, the `else` below the last, as PlantUML draws them.
    #[test]
    fn tests_stack_under_use_vertical_if() {
        let source = "@startuml\n!pragma useVerticalIf on\nstart\nif (condition A) then (yes)\n  :Text 1;\nelseif (condition B) then (yes)\n  :Text 2;\n  stop\nelseif (condition C) then (yes)\n  :Text 3;\nelse (nothing)\n  :Text else;\nendif\nstop\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        let tests = [at(3), at(5), at(8)];
        let texts = [at(4), at(6), at(9)];
        for i in 0..3 {
            assert!(
                texts[i].x > tests[i].x + tests[i].w,
                "{:?} {:?}",
                texts[i],
                tests[i]
            );
            if i > 0 {
                assert!(
                    (tests[i].x + tests[i].w / 2.0 - tests[0].x - tests[0].w / 2.0).abs() < 0.5
                );
                assert!(
                    tests[i].y > texts[i - 1].y + texts[i - 1].h,
                    "{:?} {:?}",
                    tests[i],
                    texts[i - 1]
                );
            }
        }
        let other = at(11);
        assert!(
            other.y > texts[2].y + texts[2].h && other.x < texts[2].x,
            "{other:?}"
        );
        // Off, it is the row it was.
        let off = source.replace("useVerticalIf on", "useVerticalIf off");
        let (_, map) = crate::render_mapped(&off, None).unwrap();
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        assert!((at(5).y - at(3).y).abs() < 0.5);
        let wrong = source.replace("useVerticalIf on", "useVerticalIf maybe");
        assert!(crate::render_mapped(&wrong, None).is_err());
    }

    /// Stacked, the words of every arrow are drawn and the rail keeps clear
    /// of them, as the branches keep clear of the words beside the line
    /// down; each line is followed to the diamond where they meet; a hidden
    /// or a red arrow out of the first branch hides or colors its own line,
    /// not the rail; a `while` that ends a branch goes on to the rail
    /// without turning back.
    #[test]
    fn stacked_branches_keep_their_words_and_lines() {
        let apart = |a: &crate::graph::Rect, b: &crate::graph::Rect| {
            a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y
        };
        let source = "@startuml\n' parts (Omaspec)\n!pragma useVerticalIf on\nstart\nif (a?) then (yes)\n  -> only words in a;\n(a very long incoming label here) elseif (b?) then (yes)\n  -> into B;\n  :B;\n  -> out of B;\nelseif (c?) then (yes)\n  while (more?)\n    :work;\n  endwhile\nelse\n  :C;\n  -> words after C;\nendif\nstop\n@enduml\n";
        let (svg, map) = crate::render_mapped(source, None).unwrap();
        for words in ["only words in a", "into B", "out of B", "words after C"] {
            assert!(svg.contains(words), "{words}: {svg}");
        }
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        let (incoming, b) = (at(6), at(8));
        assert!(apart(&incoming, &b), "{incoming:?} {b:?}");
        assert!(!map.wires.is_empty());
        let words: Vec<_> = [5, 7, 9, 16].map(at).into_iter().collect();
        for wire in &map.wires {
            assert!(!wire.key.to.is_empty(), "{:?}", wire.key);
            for run in wire.path.windows(2) {
                let (p, q) = (run[0], run[1]);
                let r = crate::graph::Rect {
                    x: p.0.min(q.0),
                    y: p.1.min(q.1),
                    w: (p.0 - q.0).abs(),
                    h: (p.1 - q.1).abs(),
                };
                for w in &words {
                    assert!(
                        apart(&r, w) || r.w > 0.0 && r.h == 0.0,
                        "{w:?} by {:?}",
                        wire.path
                    );
                }
            }
            // No line turns back over itself: two runs the other way along
            // the same stretch, close above each other.
            let runs: Vec<_> = wire.path.windows(2).map(|r| (r[0], r[1])).collect();
            for (i, a) in runs.iter().enumerate() {
                for b in &runs[i + 1..] {
                    let flat = |(p, q): &((f32, f32), (f32, f32))| {
                        (p.1 - q.1).abs() < 0.5 && (p.0 - q.0).abs() > 0.5
                    };
                    if flat(a) && flat(b) && (a.1.0 - a.0.0).signum() != (b.1.0 - b.0.0).signum() {
                        let (lo, hi) = (
                            a.0.0.min(a.1.0).max(b.0.0.min(b.1.0)),
                            a.0.0.max(a.1.0).min(b.0.0.max(b.1.0)),
                        );
                        assert!(
                            hi - lo < 1.0 || (a.0.1 - b.0.1).abs() > 25.0,
                            "{:?}",
                            wire.path
                        );
                    }
                }
            }
        }
        // The first branch's arrow hidden or red: the rail stays plain.
        let lines = |svg: &str| svg.matches("<polyline").count() + svg.matches("<path").count();
        let plain = "@startuml\n!pragma useVerticalIf on\nstart\nif (a?) then (yes)\n  :A;\nelseif (b?) then (yes)\n  :B;\nelse\n  :C;\nendif\nstop\n@enduml\n";
        let base = crate::render_mapped(plain, None).unwrap().0;
        let hidden =
            crate::render_mapped(&plain.replace("  :A;\n", "  :A;\n  -[hidden]->\n"), None)
                .unwrap()
                .0;
        // The hidden line, and the piece of rail it alone would take.
        assert_eq!(lines(&hidden) + 2, lines(&base), "{hidden}");
        let red = crate::render_mapped(
            &plain.replace("  :A;\n", "  :A;\n  -[#FF0000,dashed]->\n"),
            None,
        )
        .unwrap()
        .0;
        assert_eq!(red.matches("stroke=\"#FF0000\"").count(), 1, "{red}");
    }

    /// No words overlap other words or lines: those on the arrow out of an
    /// empty or a full `else` when stacked, those of several lines above an
    /// empty branch, those on the arrow out of a group below its frame.
    #[test]
    fn words_keep_clear_of_words_and_lines() {
        let stacked = "@startuml\n' parts (Omaspec)\n!pragma useVerticalIf on\nstart\nif (a?) then (yes)\n  :A;\nelseif (b?) then (yes)\n  -> first line\\nsecond line\\nthird line\\nfourth line;\nelseif (c?) then (yes)\n  :B;\n  :B2;\n  :B3;\nelse\n  -> words on the way out of an empty else that are long;\nendif\nstop\n@enduml\n";
        let full = stacked.replace(
            "else\n  -> words on the way out",
            "else\n  :C;\n  -> long words on the arrow out",
        );
        let group = "@startuml\n' parts (Omaspec)\nstart\npartition End {\n  :end;\n}\n-> this is the end;\nstop\n@enduml\n";
        for source in [stacked, full.as_str(), group] {
            let (_, map) = crate::render_mapped(source, None).unwrap();
            let rects: Vec<_> = map.tied.iter().map(|t| t.rect).collect();
            let inside = |a: &crate::graph::Rect, b: &crate::graph::Rect| {
                a.x + 1.0 < b.x + b.w
                    && b.x + 1.0 < a.x + a.w
                    && a.y + 1.0 < b.y + b.h
                    && b.y + 1.0 < a.y + a.h
            };
            for (i, a) in rects.iter().enumerate() {
                for b in &rects[i + 1..] {
                    assert!(!inside(a, b), "{source}: {a:?} {b:?}");
                }
            }
            for wire in &map.wires {
                for run in wire.path.windows(2) {
                    let (p, q) = (run[0], run[1]);
                    let r = crate::graph::Rect {
                        x: p.0.min(q.0),
                        y: p.1.min(q.1),
                        w: (p.0 - q.0).abs().max(0.01),
                        h: (p.1 - q.1).abs().max(0.01),
                    };
                    for words in &rects {
                        assert!(!inside(&r, words), "{source}: {words:?} by {:?}", wire.path);
                    }
                }
            }
        }
        // The words after a group stand below its frame.
        let (svg, map) = crate::render_mapped(group, None).unwrap();
        let words = map.tied.iter().find(|t| t.line == 6).unwrap().rect;
        let frame = svg.split("<rect").nth(1).unwrap();
        let num = |key: &str| -> f32 {
            let at = frame.find(&format!(" {key}=\"")).unwrap() + key.len() + 3;
            frame[at..].split('"').next().unwrap().parse().unwrap()
        };
        let dy: f32 = svg
            .split("translate(")
            .nth(1)
            .unwrap()
            .split(',')
            .nth(1)
            .unwrap()
            .split(')')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(words.y >= num("y") + num("height") + dy, "{words:?} {svg}");
    }

    /// `ArrowHeadColor none` draws lines without heads, but on an arrow
    /// written in a style of its own, as PlantUML; a color of their own
    /// declines.
    #[test]
    fn arrow_heads_take_none() {
        let source = "@startuml\nstart\n:Hello world;\n:Next;\nstop\n@enduml\n";
        let heads = |svg: &str| svg.matches("<polygon").count();
        let plain = crate::render_mapped(source, None).unwrap().0;
        let none = source.replace("start\n", "skinparam ArrowHeadColor none\nstart\n");
        let svg = crate::render_mapped(&none, None).unwrap().0;
        assert_eq!(heads(&plain), 3, "{plain}");
        assert_eq!(heads(&svg), 0, "{svg}");
        assert_eq!(
            plain.matches("<polyline").count(),
            svg.matches("<polyline").count()
        );
        for own in ["-[#FF0000]->", "-[dashed]->", "-[bold]->", "-[#181818]->"] {
            let marked = none.replace(":Next;", &format!("{own}\n:Next;"));
            let svg = crate::render_mapped(&marked, None).unwrap().0;
            assert_eq!(heads(&svg), 1, "{own}: {svg}");
        }
        let red = source.replace("start\n", "skinparam ArrowHeadColor #FF0000\nstart\n");
        assert!(crate::render_mapped(&red, None).is_err());
    }

    /// Under `conditionStyle diamond`, the words on the arrow into a branch
    /// stand above the test it starts with; a `repeat`'s test keeps below
    /// its body and its `backward` action right of the test, and its ways
    /// out have no words, as PlantUML draws it.
    #[test]
    fn words_keep_clear_of_a_test_beside_its_diamond() {
        let apart = |a: &crate::graph::Rect, b: &crate::graph::Rect| {
            a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y
        };
        let tied = |source: &str, line: usize| {
            let (_, map) = crate::render_mapped(source, None).unwrap();
            map.tied
                .iter()
                .filter(|t| t.line == line)
                .map(|t| t.rect)
                .collect::<Vec<_>>()
        };
        let nested = "@startuml\nskinparam conditionStyle diamond\nstart\nif (outer test?) then (yes)\n-> some words;\nif (inner test here?) then (y)\n:a;\nendif\nelse (no)\n:b;\nendif\nstop\n@enduml\n";
        let (words, test) = (tied(nested, 4)[0], tied(nested, 5)[0]);
        assert!(
            apart(&words, &test) && words.y + words.h <= test.y + 0.5,
            "{words:?} {test:?}"
        );
        let tall = "@startuml\nskinparam conditionStyle diamond\nrepeat\n:wide action in the body here;\nrepeat while (line one\\nline two\\nline three?) is (yes)\n@enduml\n";
        let (body, test) = (tied(tall, 3)[0], tied(tall, 4)[0]);
        assert!(apart(&body, &test), "{body:?} {test:?}");
        let back = "@startuml\nskinparam conditionStyle diamond\nrepeat\n:a;\nbackward :log;\nrepeat while (is the queue still not empty?) is (yes) not (no)\n@enduml\n";
        let (log, test) = (tied(back, 4)[0], tied(back, 5));
        assert_eq!(test.len(), 1, "no words on the ways out: {test:?}");
        assert!(log.x > test[0].x + test[0].w, "{log:?} {test:?}");
    }

    /// `conditionStyle diamond`: the test stands above an empty diamond,
    /// right of the line into it, as PlantUML draws it.
    #[test]
    fn a_test_stands_beside_an_empty_diamond() {
        let source = "@startuml\nskinparam conditionStyle diamond\nstart\nif (is it ready?) then (yes)\n:go;\nelse (no)\n:wait;\nendif\nstop\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        let (test, go, wait) = (at(3), at(4), at(6));
        let middle = (go.x + go.w / 2.0 + wait.x + wait.w / 2.0) / 2.0;
        assert!(
            test.x > middle && test.y + test.h + 24.0 < go.y,
            "{test:?} {go:?} {wait:?}"
        );
    }

    /// With a `backward`, a `while` goes back on the right through its
    /// action and out on the left, the words of its way out left of the
    /// diamond, as PlantUML draws it.
    #[test]
    fn a_whiles_backward_stands_on_the_right() {
        let source = "@startuml\nwhile (check filesize ?) is (not empty)\n  :read file;\n  backward:log;\nendwhile (empty)\n:close file;\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        let (test, body, back, out) = (at(1), at(2), at(3), at(4));
        assert!(
            back.x > body.x + body.w && back.x > test.x + test.w,
            "{back:?} {body:?}"
        );
        assert!(out.x + out.w <= test.x, "{out:?} {test:?}");
        // The action keeps clear of the words beside the line into the
        // body: its `yes`, the words on its first arrow.
        let apart = |a: &crate::graph::Rect, b: &crate::graph::Rect| {
            a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y
        };
        for (source, words, back) in [
            (
                "@startuml\nstart\nwhile (x?)\n-> a very long arrow label words;\n:a;\nbackward :log;\nendwhile\n:after;\n@enduml\n",
                3,
                5,
            ),
            (
                "@startuml\nstart\nwhile (x?) is (a fairly long yes label)\n:a;\nbackward :log;\nendwhile\n:after;\n@enduml\n",
                2,
                4,
            ),
        ] {
            let (_, map) = crate::render_mapped(source, None).unwrap();
            let rect = |line: usize| {
                let found = map.tied.iter().filter(|t| t.line == line);
                found.map(|t| t.rect).collect::<Vec<_>>()
            };
            let back = rect(back)[0];
            for words in rect(words) {
                assert!(apart(&words, &back), "{source}: {words:?} {back:?}");
            }
        }
    }

    /// Cases of a `switch` that run on into another swimlane keep clear of
    /// one another there; words on an arrow in one lane do not widen the
    /// next.
    #[test]
    fn branches_across_swimlanes_keep_clear() {
        let source = "@startuml\n|A|\n|B|\nstart\nswitch (x?)\ncase (1)\n  |A|\n  :a1;\n  |B|\n  :a2 wide action here;\n  :a3 wider action here too;\ncase (2)\n  |B|\n  :a very very very wide left case action;\n  :second;\n  :third wide step of the left case;\ncase (3)\n  :c;\ncase (4)\n  :d;\nendswitch\nstop\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let boxes: Vec<_> = [7, 9, 10, 13, 14, 15, 17, 19]
            .iter()
            .map(|&line| map.tied.iter().find(|t| t.line == line).unwrap().rect)
            .collect();
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                let apart =
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{a:?} {b:?}");
            }
        }
        let lanes = "@startuml\n|A|\n|B|\n|A|\nstart\nif (x?) then (yes)\n-> some long words on the arrow;\n:a;\nelse (no)\n|B|\n:b;\nendif\nstop\n@enduml\n";
        let after = |source: &str| {
            let (svg, map) = crate::render_mapped(source, None).unwrap();
            let width: f32 = svg
                .split("width=\"")
                .nth(1)
                .unwrap()
                .split('"')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            let b = map.tied.iter().find(|t| t.line == 10).unwrap().rect;
            width - b.x
        };
        // In a row of `elseif`s, whose flows come in the order written.
        let row = "@startuml\n|A|\n|B|\n|A|\nstart\nif (a?) then (yes)\n-> a very long arrow label with many words;\n:a;\nelseif (b?) then (yes)\n|B|\n:b;\nelse\n|A|\n:c;\nendif\nstop\n@enduml\n";
        let (_, map) = crate::render_mapped(row, None).unwrap();
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        let (words, c) = (at(6), at(13));
        assert!(words.x + words.w < c.x + c.w / 2.0, "{words:?} {c:?}");
        let plain = lanes.replace("-> some long words on the arrow;\n", "-> on;\n");
        assert!(
            (after(lanes) - after(&plain)).abs() < 0.5,
            "{} {}",
            after(lanes),
            after(&plain)
        );
    }

    /// Each case's words stand beside its line, tied to its line, in the
    /// order of the cases and clear of each other and of the diamond.
    #[test]
    fn a_switchs_cases_stand_side_by_side() {
        let source = "@startuml\nstart\nswitch (what kind of order is it?)\ncase (online)\n  :Ship;\ncase (in store)\ncase (by phone)\n  :Call;\ncase (fax)\n  :Read;\nendswitch\nstop\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        let test = at(2);
        let words: Vec<_> = [3, 5, 6, 8].map(at).into_iter().collect();
        let apart = |a: &crate::graph::Rect, b: &crate::graph::Rect| {
            a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y
        };
        for (i, a) in words.iter().enumerate() {
            assert!(apart(a, &test), "{a:?} {test:?}");
            for b in &words[i + 1..] {
                assert!(apart(a, b) && a.x < b.x, "{a:?} {b:?}");
            }
        }
        // The middle case goes straight down from the foot, its long words
        // clear of the line to the case right of it; words on the arrows
        // into the cases keep clear of the next case's line too.
        let source = "@startuml\nstart\nswitch (x)\ncase (a)\n  -> a long arrow label here;\n  :A;\ncase (a very long middle case words here)\n  -> another long arrow label;\n  :B;\ncase (c)\n  :C;\nendswitch\nstop\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        let (a_arrow, middle, b_arrow, b, c, c_box) = (at(4), at(6), at(7), at(8), at(9), at(10));
        assert!(a_arrow.x + a_arrow.w + 6.0 < b.x, "{a_arrow:?} {b:?}");
        for left in [middle, b_arrow] {
            assert!(left.x + left.w + 6.0 < c.x - 6.0, "{left:?} {c:?}");
        }
        assert!(c_box.x > middle.x + middle.w, "{c_box:?} {middle:?}");
    }

    /// The fork bars of `svg` on the page, as the map has it: left end,
    /// top, width.
    fn bars(svg: &str) -> Vec<(f32, f32, f32)> {
        let shift = svg
            .split("translate(")
            .nth(1)
            .unwrap()
            .split(')')
            .next()
            .unwrap();
        let (dx, dy) = shift.split_once(',').unwrap();
        let (dx, dy): (f32, f32) = (dx.parse().unwrap(), dy.parse().unwrap());
        svg.split("<rect")
            .skip(1)
            .filter(|r| r.contains("fill=\"#555555\""))
            .map(|bar| {
                let num = |key: &str| -> f32 {
                    let at = bar.find(&format!(" {key}=\"")).unwrap() + key.len() + 3;
                    bar[at..].split('"').next().unwrap().parse().unwrap()
                };
                (num("x") + dx, num("y") + dy, num("width"))
            })
            .collect()
    }

    /// The words of `end fork {or}` stand right of the bar that joins the
    /// flows: one drawn when every flow ends before it, as PlantUML draws
    /// it, and in swimlanes beside its rightmost end, not on it. What
    /// follows such a bar goes on from it.
    #[test]
    fn end_fork_words_stand_right_of_the_join() {
        let ended = "@startuml\nstart\nfork\n:a;\ndetach\nfork again\n:b;\ndetach\nend fork {or}\n@enduml\n";
        let lanes = "@startuml\n|A|\nstart\nfork\n:a;\nfork again\n|B|\n:b;\nend fork {or}\n|A|\n:after;\nstop\n@enduml\n";
        for (source, line) in [(ended, 8), (lanes, 8)] {
            let (svg, map) = crate::render_mapped(source, None).unwrap();
            let words = map.tied.iter().find(|t| t.line == line).unwrap().rect;
            let bars = bars(&svg);
            let low = bars.iter().map(|b| b.1).fold(f32::NEG_INFINITY, f32::max);
            assert!(bars.iter().any(|b| b.1 == low && b.1 > bars[0].1), "{svg}");
            let right = bars
                .iter()
                .filter(|b| b.1 == low)
                .map(|b| b.0 + b.2)
                .fold(0.0, f32::max);
            assert!(words.x >= right, "{words:?} {bars:?}");
            assert!(
                (words.y + words.h / 2.0 - (low + 3.0)).abs() < 4.0,
                "{words:?} {bars:?}"
            );
        }
        // Without words, the bar is there all the same; a step after it
        // has an arrow from it.
        let (svg, _) = crate::render_mapped(&ended.replace(" {or}", ""), None).unwrap();
        assert_eq!(bars(&svg).len(), 2, "{svg}");
        assert_eq!(svg.matches("<polygon").count(), 3, "{svg}");
        let after = ended.replace("end fork {or}\n", "end fork\n:after;\n");
        let (svg, _) = crate::render_mapped(&after, None).unwrap();
        assert_eq!(svg.matches("<polygon").count(), 4, "{svg}");
    }

    /// A connector is a circle with its name, a place of the map; a flow
    /// that ends at `kill` or `detach` has no arrow on.
    #[test]
    fn a_connector_is_a_circle_and_detach_ends_the_flow() {
        let source =
            "@startuml\nstart\n:Some activity;\n(A)\ndetach\n(A)\n:Other activity;\n@enduml\n";
        let (svg, map) = crate::render_mapped(source, None).unwrap();
        assert_eq!(svg.matches("<ellipse").count(), 3, "{svg}");
        assert!(map.tied.iter().any(|t| t.line == 3), "{:?}", map.tied);
        let arrows = |svg: &str| svg.matches("<polygon").count();
        let ended = crate::render_mapped("@startuml\nstart\n:A;\nkill\n@enduml\n", None)
            .unwrap()
            .0;
        let open = crate::render_mapped("@startuml\nstart\n:A;\n@enduml\n", None)
            .unwrap()
            .0;
        assert_eq!(arrows(&ended), arrows(&open), "{ended}");
    }

    /// A note left of the action on a loop's way back stands between the
    /// loop and that action, clear of both.
    #[test]
    fn a_note_left_of_backward_keeps_off_the_loop() {
        let source = "@startuml\nrepeat :a;\n:b;\nbackward :back;\nnote left: a long note on the left of it all\nrepeat while (x?) is (yes) not (no)\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let at = |line: usize| map.tied.iter().find(|t| t.line == line).unwrap().rect;
        let (body, back, note) = (at(2), at(3), at(4));
        assert!(note.x > body.x + body.w, "{note:?} {body:?}");
        assert!(note.x + note.w < back.x, "{note:?} {back:?}");
    }

    /// A hidden arrow is no line of the picture or of the map; its words
    /// stay, as PlantUML draws them. A split whose flows all start hidden
    /// has no line on top; a fork keeps its bar. A `while` at the end of a
    /// branch whose last arrow is hidden has no way out.
    #[test]
    fn a_hidden_arrow_draws_nothing() {
        let source = "@startuml\nsplit\n-[hidden]->\n:A;\nsplit again\n-[hidden]-> quiet;\n:B;\nend split\n:D;\n@enduml\n";
        let (svg, _) = crate::render_mapped(source, None).unwrap();
        assert!(svg.contains("quiet"), "{svg}");
        let plain = source.replace("-[hidden]->", "->");
        let (plain_svg, _) = crate::render_mapped(&plain, None).unwrap();
        // Lines, heads and the bar on top.
        let drawn = |svg: &str| {
            ["<path", "<polyline", "<polygon", "<line", "<rect"]
                .iter()
                .map(|tag| svg.matches(tag).count())
                .sum::<usize>()
        };
        // Two lines, two heads and the line on top fewer.
        assert_eq!(drawn(&plain_svg) - drawn(&svg), 5, "{svg}");
        // A fork: the bar stays.
        let fork = source
            .replace("split again", "fork again")
            .replace("end split", "end fork")
            .replacen("split", "fork", 1);
        let plain_fork = fork.replace("-[hidden]->", "->");
        let (fork_svg, plain_fork_svg) = (
            crate::render_mapped(&fork, None).unwrap().0,
            crate::render_mapped(&plain_fork, None).unwrap().0,
        );
        assert_eq!(drawn(&plain_fork_svg) - drawn(&fork_svg), 4, "{fork_svg}");
        // A `while` at a branch's end, its last arrow hidden.
        let ended = "@startuml\nif (ok?) then (yes)\n  while (more?)\n    :work;\n  endwhile\n  -[hidden]->\nelse (no)\n  :other;\nendif\n@enduml\n";
        let shown = ended.replace("  -[hidden]->\n", "");
        let (ended_svg, shown_svg) = (
            crate::render_mapped(ended, None).unwrap().0,
            crate::render_mapped(&shown, None).unwrap().0,
        );
        assert!(drawn(&ended_svg) < drawn(&shown_svg), "{ended_svg}");
    }

    use crate::render;

    #[test]
    fn lines_round_their_corners_unless_ortho() {
        let body = "start\nif (ok?) then (yes)\n:a;\nelse (no)\n:b long;\nendif\nstop";
        // A rounded corner is a cubic in a path; a right angle is not.
        let curves = |svg: &str| svg.matches(" C").count();
        let round = render(body).unwrap();
        assert!(curves(&round) > 0, "{round}");
        let ortho = render(&format!("skinparam linetype ortho\n{body}")).unwrap();
        assert_eq!(curves(&ortho), 0, "{ortho}");
    }

    #[test]
    fn an_elseif_label_stands_off_the_way_its_line_turns() {
        // The `bug` branch turns right below its diamond toward the next
        // swimlane: its label stands left of the way down.
        let svg = render(
            "|Support|\nstart\nif (Known issue?) then (yes)\n:Send article;\nelseif (Bug?) then (bug)\n|Engineering|\n:Reproduce;\n|Support|\nelse (no)\n:Ask;\nendif\nstop",
        )
        .unwrap();
        let x_of = |words: &str| -> f32 {
            svg.split("<tspan ")
                .find(|tspan| tspan.contains(&format!(">{words}<")))
                .and_then(|tspan| tspan.split("x=\"").nth(1)?.split('"').next()?.parse().ok())
                .unwrap()
        };
        let label = x_of("bug");
        let diamond = x_of("Bug?");
        assert!(label < diamond, "{label} {diamond}");
        // Long labels between two ways down, one right of the first and
        // one left of the next, stand apart.
        let svg = render(
            "|Support|\nstart\nif (Known?) then (customer already reported)\n:Send;\nelseif (Bug?) then (engineering must reproduce)\n|Engineering|\n:Reproduce;\n|Support|\nelse (no)\n:Ask;\nendif\nstop",
        )
        .unwrap();
        let span = |words: &str| -> (f32, f32) {
            let tspan = svg
                .split("<tspan ")
                .find(|tspan| tspan.contains(&format!(">{words}<")))
                .unwrap();
            let number = |name: &str| -> f32 {
                tspan
                    .split(&format!(" {name}=\""))
                    .nth(1)
                    .or_else(|| tspan.strip_prefix(&format!("{name}=\"")))
                    .and_then(|rest| rest.split('"').next()?.parse().ok())
                    .unwrap()
            };
            (number("x"), number("x") + number("textLength"))
        };
        let (first, second) = (
            span("customer already reported"),
            span("engineering must reproduce"),
        );
        assert!(first.1 < second.0, "{first:?} {second:?}");
    }

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
    fn a_test_inside_its_diamond_is_wider_than_high() {
        // A short test, and a long one that keeps its own shape.
        for (test, least, most) in [("ok?", 1.59, 1.61), ("is the address complete?", 1.6, 9.0)] {
            let svg = render(&format!(
                "skinparam conditionStyle InsideDiamond\nstart\nif ({test}) then (yes)\n:go;\nendif\nstop"
            ))
            .unwrap();
            // The largest polygon of four corners: the diamond, not a head.
            let (w, h) = svg
                .split("<polygon points=\"")
                .skip(1)
                .filter_map(|rest| {
                    let points: Vec<(f32, f32)> = rest
                        .split('"')
                        .next()?
                        .split(' ')
                        .filter_map(|p| {
                            let (x, y) = p.split_once(',')?;
                            Some((x.parse().ok()?, y.parse().ok()?))
                        })
                        .collect();
                    let xs = points.iter().map(|p| p.0);
                    let ys = points.iter().map(|p| p.1);
                    let w = xs.clone().fold(f32::MIN, f32::max) - xs.fold(f32::MAX, f32::min);
                    let h = ys.clone().fold(f32::MIN, f32::max) - ys.fold(f32::MAX, f32::min);
                    (points.len() == 4).then_some((w, h))
                })
                .max_by(|a, b| (a.0 * a.1).total_cmp(&(b.0 * b.1)))
                .expect("a diamond");
            let ratio = w / h;
            assert!((least..=most).contains(&ratio), "{test}: {w} x {h}");
        }
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
