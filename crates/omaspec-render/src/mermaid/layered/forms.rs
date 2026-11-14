//! The shapes Mermaid 11 names in `A@{ shape: doc }` beyond those mmdr
//! reads from brackets: a document, a card, a delay, a triangle, stacked
//! documents and the like. mmdr has no shape for them and drew the
//! `shape: …` text itself as the label; each is outlined and painted here.
//!
//! Each box is measured from its words' room, the words with their
//! padding, so the words lie inside the outline; the outline's curves and
//! slants follow from the box alone. The small marks (a start, a stop, a
//! junction, a fork, a summary, a lightning bolt) show no words, as in
//! Mermaid.

use omaspec_uml::graph::Rect;
use omaspec_uml::{Canvas, Layer, Paint, Stroke};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Form {
    /// `notch-rect`, `card`: the top left corner cut off.
    Card,
    /// `lin-rect`: a line down its left side.
    LinedRect,
    /// `sm-circ`, `start`.
    SmallCircle,
    /// `fr-circ`, `stop`: a ring around a dot.
    Stop,
    /// `f-circ`, `junction`.
    Junction,
    /// `fork`, `join`: a bar.
    Fork,
    /// `hourglass`, `collate`: its words below it.
    Hourglass,
    /// `brace`, `comment`: words with a brace on the left.
    BraceLeft,
    /// `brace-r`.
    BraceRight,
    /// `braces`: a brace on each side.
    Braces,
    /// `bolt`, `com-link`.
    Bolt,
    /// `doc`: a wave along the bottom.
    Document,
    /// `delay`: the right side a half circle.
    Delay,
    /// `h-cyl`, `das`: a cylinder on its side.
    LyingCylinder,
    /// `lin-cyl`, `disk`: a cylinder with a second rim.
    Disk,
    /// `curv-trap`, `display`: a point on the left, round on the right.
    Display,
    /// `div-rect`: a line across near the top.
    Divided,
    /// `tri`, `extract`.
    Triangle,
    /// `flip-tri`, `manual-file`.
    FlippedTriangle,
    /// `win-pane`, `internal-storage`.
    WindowPane,
    /// `lin-doc`.
    LinedDocument,
    /// `notch-pent`, `loop-limit`: both top corners cut off.
    LoopLimit,
    /// `sl-rect`, `manual-input`: the top sloping up to the right.
    ManualInput,
    /// `docs`, `documents`: three documents stacked.
    Documents,
    /// `processes`: three boxes stacked.
    Processes,
    /// `flag`, `paper-tape`: a wave along the top and the bottom.
    PaperTape,
    /// `bow-rect`, `stored-data`.
    StoredData,
    /// `cross-circ`, `summary`.
    Summary,
    /// `tag-doc`: a document with a tag in its corner.
    TaggedDocument,
    /// `tag-rect`: a box with a tag in its corner.
    TaggedProcess,
}

/// How far a brace, a lined side or a pane's lines stand in.
const INSET: f32 = 8.0;
/// How far stacked copies step up and right.
const STACK: f32 = 5.0;
/// The side of the tag in a corner.
const TAG: f32 = 10.0;
/// How high an hourglass is, above its words.
const GLASS: f32 = 30.0;
/// Of a manual input's height, how much its top slopes.
const SLOPE: f32 = 0.23;

impl Form {
    /// The form a Mermaid shape name, or one of its aliases, names.
    pub(super) fn named(name: &str) -> Option<Form> {
        Some(match name {
            "notch-rect" | "card" | "notched-rectangle" => Form::Card,
            "lin-rect" | "lin-proc" | "lined-process" | "lined-rectangle" | "shaded-process" => {
                Form::LinedRect
            }
            "sm-circ" | "small-circle" | "start" => Form::SmallCircle,
            "fr-circ" | "framed-circle" | "stop" => Form::Stop,
            "f-circ" | "filled-circle" | "junction" => Form::Junction,
            "fork" | "join" => Form::Fork,
            "hourglass" | "collate" => Form::Hourglass,
            "brace" | "brace-l" | "comment" => Form::BraceLeft,
            "brace-r" => Form::BraceRight,
            "braces" => Form::Braces,
            "bolt" | "com-link" | "lightning-bolt" => Form::Bolt,
            "doc" | "document" => Form::Document,
            "delay" | "half-rounded-rectangle" => Form::Delay,
            "h-cyl" | "das" | "horizontal-cylinder" => Form::LyingCylinder,
            "lin-cyl" | "disk" | "lined-cylinder" => Form::Disk,
            "curv-trap" | "curved-trapezoid" | "display" => Form::Display,
            "div-rect" | "div-proc" | "divided-process" | "divided-rectangle" => Form::Divided,
            "tri" | "extract" | "triangle" => Form::Triangle,
            "flip-tri" | "flipped-triangle" | "manual-file" => Form::FlippedTriangle,
            "win-pane" | "internal-storage" | "window-pane" => Form::WindowPane,
            "lin-doc" | "lined-document" => Form::LinedDocument,
            "notch-pent" | "loop-limit" | "notched-pentagon" => Form::LoopLimit,
            "sl-rect" | "manual-input" | "sloped-rectangle" => Form::ManualInput,
            "docs" | "documents" | "st-doc" | "stacked-document" => Form::Documents,
            "processes" | "procs" | "st-rect" | "stacked-rectangle" => Form::Processes,
            "flag" | "paper-tape" => Form::PaperTape,
            "bow-rect" | "bow-tie-rectangle" | "stored-data" => Form::StoredData,
            "cross-circ" | "crossed-circle" | "summary" => Form::Summary,
            "tag-doc" | "tagged-document" => Form::TaggedDocument,
            "tag-rect" | "tag-proc" | "tagged-process" | "tagged-rectangle" => Form::TaggedProcess,
            _ => return None,
        })
    }

    /// Whether its words are drawn: not in the small marks.
    pub(super) fn shows_text(self) -> bool {
        !matches!(
            self,
            Form::SmallCircle
                | Form::Stop
                | Form::Junction
                | Form::Fork
                | Form::Bolt
                | Form::Summary
        )
    }

    /// Whether a line may end anywhere along its box's side: a brace has
    /// no outline of its own.
    pub(super) fn flat(self) -> bool {
        matches!(self, Form::BraceLeft | Form::BraceRight | Form::Braces)
    }

    /// The box for words whose room, padding and all, is `(w, h)`; `text`
    /// is the words alone.
    pub(super) fn size(self, (w, h): (f32, f32), text: (f32, f32)) -> (f32, f32) {
        match self {
            Form::SmallCircle => (14.0, 14.0),
            Form::Stop => (18.0, 18.0),
            Form::Junction => (12.0, 12.0),
            Form::Fork => (70.0, 8.0),
            Form::Bolt => (22.0, 34.0),
            Form::Summary => (24.0, 24.0),
            Form::Hourglass => ((text.0 + 12.0).max(GLASS), GLASS + 12.0 + text.1),
            Form::BraceLeft | Form::BraceRight | Form::LinedRect => (w + INSET, h),
            Form::Braces => (w + 2.0 * INSET, h),
            Form::Document | Form::TaggedDocument => (w, document_height(h)),
            Form::LinedDocument => (w + INSET, document_height(h)),
            Form::PaperTape => (w, h * 1.3 + 12.0),
            Form::Delay => (w + h / 2.0, h),
            Form::LyingCylinder => (w + 3.0 * lying_cap(h), h),
            Form::Disk => (w, h + 3.0 * super::shape::cap(w)),
            Form::Display => (w + h * 0.75, h),
            Form::Divided => (w, h + INSET),
            Form::Triangle | Form::FlippedTriangle => (2.0 * w, 2.0 * h),
            Form::WindowPane => (w + INSET, h + INSET),
            Form::ManualInput => (w, h / (1.0 - SLOPE)),
            Form::Documents => (w + 2.0 * STACK, document_height(h) + 2.0 * STACK),
            Form::Processes => (w + 2.0 * STACK, h + 2.0 * STACK),
            Form::StoredData => (w + 0.4 * h, h),
            Form::Card | Form::LoopLimit | Form::TaggedProcess => (w, h),
        }
    }

    /// The middle of where the words go.
    pub(super) fn text_center(self, r: Rect) -> (f32, f32) {
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        match self {
            Form::Hourglass => (cx, r.y + GLASS + 6.0 + (r.h - GLASS - 12.0) / 2.0),
            Form::LinedRect | Form::BraceLeft => (cx + INSET / 2.0, cy),
            Form::BraceRight => (cx - INSET / 2.0, cy),
            Form::Document | Form::TaggedDocument => (cx, r.y + (r.h - dip(r.h)) / 2.0),
            Form::LinedDocument => (cx + INSET / 2.0, r.y + (r.h - dip(r.h)) / 2.0),
            Form::Delay => (cx - r.h / 4.0, cy),
            Form::LyingCylinder => (cx - lying_cap(r.h) / 2.0, cy),
            Form::Disk => (cx, cy + 1.5 * super::shape::cap(r.w)),
            Form::Display => (cx - r.h / 8.0, cy),
            Form::Divided => (cx, cy + INSET / 2.0),
            Form::Triangle => (cx, r.y + r.h * 0.75),
            Form::FlippedTriangle => (cx, r.y + r.h * 0.25),
            Form::WindowPane => (cx + INSET / 2.0, cy + INSET / 2.0),
            Form::ManualInput => (cx, r.y + r.h * (1.0 + SLOPE) / 2.0),
            Form::Documents => {
                let front = front(r);
                (
                    front.x + front.w / 2.0,
                    front.y + (front.h - dip(front.h)) / 2.0,
                )
            }
            Form::Processes => (cx - STACK, cy + STACK),
            _ => (cx, cy),
        }
    }

    /// The outline lines end on, as a polygon.
    pub(super) fn outline(self, r: Rect) -> Vec<(f32, f32)> {
        let (x, y, w, h) = (r.x, r.y, r.w, r.h);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        match self {
            Form::SmallCircle | Form::Stop | Form::Junction | Form::Summary => {
                arc((cx, cy), (w / 2.0, h / 2.0), 0.0, 360.0)
            }
            Form::Fork
            | Form::BraceLeft
            | Form::BraceRight
            | Form::Braces
            | Form::LinedRect
            | Form::WindowPane
            | Form::Divided
            | Form::TaggedProcess
            // Lines meet the glass and its words below it as one box.
            | Form::Hourglass => box_of(r),
            Form::Bolt => vec![
                (x + w * 0.55, y),
                (x + w * 0.1, y + h * 0.55),
                (x + w * 0.5, y + h * 0.55),
                (x + w * 0.35, y + h),
                (x + w * 0.9, y + h * 0.42),
                (x + w * 0.5, y + h * 0.42),
                (x + w * 0.75, y),
            ],
            Form::Document | Form::TaggedDocument | Form::LinedDocument => document(r),
            Form::Card => {
                let c = (h / 3.0).min(12.0);
                vec![(x + c, y), (x + w, y), (x + w, y + h), (x, y + h), (x, y + c)]
            }
            Form::LoopLimit => {
                let c = (h / 3.0).min(12.0);
                vec![
                    (x + c, y),
                    (x + w - c, y),
                    (x + w, y + c),
                    (x + w, y + h),
                    (x, y + h),
                    (x, y + c),
                ]
            }
            Form::Delay => {
                let radius = h / 2.0;
                let mut points = vec![(x, y)];
                points.extend(arc((x + w - radius, cy), (radius, radius), -90.0, 90.0));
                points.push((x, y + h));
                points
            }
            Form::LyingCylinder => {
                let rx = lying_cap(h);
                let mut points = arc((x + w - rx, cy), (rx, h / 2.0), -90.0, 90.0);
                points.extend(arc((x + rx, cy), (rx, h / 2.0), 90.0, 270.0));
                points
            }
            Form::Disk => {
                let ry = super::shape::cap(w);
                let mut points = arc((cx, y + ry), (w / 2.0, ry), 180.0, 360.0);
                points.extend(arc((cx, y + h - ry), (w / 2.0, ry), 0.0, 180.0));
                points
            }
            Form::Display => {
                let radius = h / 2.0;
                let mut points = vec![(x + h / 4.0, y)];
                points.extend(arc((x + w - radius, cy), (radius, radius), -90.0, 90.0));
                points.push((x + h / 4.0, y + h));
                points.push((x, cy));
                points
            }
            Form::Triangle => vec![(cx, y), (x + w, y + h), (x, y + h)],
            Form::FlippedTriangle => vec![(x, y), (x + w, y), (cx, y + h)],
            Form::ManualInput => vec![(x, y + h * SLOPE), (x + w, y), (x + w, y + h), (x, y + h)],
            Form::Documents => document(front(r)),
            Form::Processes => box_of(front(r)),
            Form::PaperTape => {
                let a = (h * 0.06).clamp(3.0, 8.0);
                let mut points = wave((x, y + a), (x + w, y + a), a);
                points.extend(wave((x + w, y + h - a), (x, y + h - a), a));
                points
            }
            Form::StoredData => {
                let bow = h * 0.2;
                let mut points = vec![(x + bow, y), (x + w, y)];
                points.extend(arc((x + w, cy), (bow, h / 2.0), -90.0, -270.0));
                points.push((x + bow, y + h));
                points.extend(arc((x + bow, cy), (bow, h / 2.0), 90.0, 270.0));
                points
            }
        }
    }

    /// Paints it in `fill` and `border`.
    pub(super) fn paint(self, canvas: &mut Canvas, r: Rect, fill: Paint, border: Stroke) {
        let layer = Layer::Frames;
        let (x, y, w, h) = (r.x, r.y, r.w, r.h);
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let mut line = |points: &[(f32, f32)]| canvas.polyline(layer, points, border);
        match self {
            Form::BraceLeft => line(&brace(x, (y, h), 1.0)),
            Form::BraceRight => line(&brace(x + w, (y, h), -1.0)),
            Form::Braces => {
                line(&brace(x, (y, h), 1.0));
                line(&brace(x + w, (y, h), -1.0));
            }
            Form::Hourglass => {
                let (left, right) = (cx - GLASS / 2.0, cx + GLASS / 2.0);
                let waist = (cx, y + GLASS / 2.0);
                let glass = [
                    (left, y),
                    (right, y),
                    waist,
                    (right, y + GLASS),
                    (left, y + GLASS),
                    waist,
                ];
                canvas.polygon(layer, &glass, fill, Some(border));
            }
            Form::Junction | Form::Fork => {
                canvas.polygon(layer, &self.outline(r), Some(border.color), Some(border));
            }
            Form::Stop => {
                canvas.polygon(layer, &self.outline(r), fill, Some(border));
                canvas.ellipse(
                    layer,
                    (cx, cy),
                    (w / 4.0, h / 4.0),
                    Some(border.color),
                    None,
                );
            }
            Form::Documents | Form::Processes => {
                for step in [2.0, 1.0, 0.0] {
                    let behind = Rect {
                        x: x + step * STACK,
                        y: y + (2.0 - step) * STACK,
                        ..front(r)
                    };
                    let outline = if self == Form::Documents {
                        document(behind)
                    } else {
                        box_of(behind)
                    };
                    canvas.polygon(layer, &outline, fill, Some(border));
                }
            }
            _ => canvas.polygon(layer, &self.outline(r), fill, Some(border)),
        }
        // What lies on the outline's face.
        let mut line = |points: &[(f32, f32)]| canvas.polyline(layer, points, border);
        match self {
            Form::LinedRect => line(&[(x + INSET, y), (x + INSET, y + h)]),
            Form::LinedDocument => line(&[(x + INSET, y), (x + INSET, y + h - dip(h))]),
            Form::WindowPane => {
                line(&[(x + INSET, y), (x + INSET, y + h)]);
                line(&[(x, y + INSET), (x + w, y + INSET)]);
            }
            Form::Divided => line(&[(x, y + INSET), (x + w, y + INSET)]),
            Form::Summary => {
                let d = w / 2.0 * std::f32::consts::FRAC_1_SQRT_2;
                line(&[(cx - d, cy - d), (cx + d, cy + d)]);
                line(&[(cx - d, cy + d), (cx + d, cy - d)]);
            }
            Form::LyingCylinder => {
                let rx = lying_cap(h);
                line(&arc((x + w - rx, cy), (rx, h / 2.0), 90.0, 270.0));
            }
            Form::Disk => {
                let ry = super::shape::cap(w);
                line(&arc((cx, y + ry), (w / 2.0, ry), 0.0, 180.0));
                line(&arc((cx, y + 2.0 * ry), (w / 2.0, ry), 0.0, 180.0));
            }
            Form::TaggedProcess | Form::TaggedDocument => {
                let bottom = if self == Form::TaggedDocument {
                    y + h - dip(h) / 2.0
                } else {
                    y + h
                };
                let tag = [
                    (x + w - TAG, bottom),
                    (x + w, bottom),
                    (x + w, bottom - TAG),
                ];
                canvas.polygon(layer, &tag, Some(border.color), None);
            }
            _ => {}
        }
    }
}

/// A document's box for words `h` high: room under them for its wave.
fn document_height(h: f32) -> f32 {
    h * 1.15 + 4.0
}

/// How deep the wave of a document `h` high dips: what
/// [`document_height`] left room for.
fn dip(h: f32) -> f32 {
    (h * 0.1).clamp(4.0, 12.0)
}

/// How far a cylinder on its side curves in at each end.
fn lying_cap(h: f32) -> f32 {
    (h / 5.0).max(6.0)
}

/// The front of a stack in `r`: at the bottom left, the copies above and
/// right of it.
fn front(r: Rect) -> Rect {
    Rect {
        x: r.x,
        y: r.y + 2.0 * STACK,
        w: r.w - 2.0 * STACK,
        h: r.h - 2.0 * STACK,
    }
}

fn box_of(r: Rect) -> Vec<(f32, f32)> {
    vec![
        (r.x, r.y),
        (r.x + r.w, r.y),
        (r.x + r.w, r.y + r.h),
        (r.x, r.y + r.h),
    ]
}

/// A document in `r`: a box whose bottom is one wave.
fn document(r: Rect) -> Vec<(f32, f32)> {
    let a = dip(r.h) / 2.0;
    let base = r.y + r.h - a;
    let mut points = vec![(r.x, r.y), (r.x + r.w, r.y)];
    points.extend(wave((r.x + r.w, base), (r.x, base), a));
    points
}

/// A level line from `from` to `to` that swings `a` down and up, once.
fn wave(from: (f32, f32), to: (f32, f32), a: f32) -> Vec<(f32, f32)> {
    const STEPS: usize = 24;
    (0..=STEPS)
        .map(|step| {
            let t = step as f32 / STEPS as f32;
            (
                from.0 + (to.0 - from.0) * t,
                from.1 + (to.1 - from.1) * t + a * (t * std::f32::consts::TAU).sin(),
            )
        })
        .collect()
}

/// A brace at `x`, from `y` down `h`, its point at the middle towards the
/// outside and its ends turned in by [`INSET`]; `inward` is 1 when inside
/// is to the right.
fn brace(x: f32, (y, h): (f32, f32), inward: f32) -> Vec<(f32, f32)> {
    let d = INSET / 2.0;
    let r = d.min(h / 4.0);
    // Drawn as a left brace at 0, then turned to face `inward`.
    let mut points = arc((2.0 * d, r), (d, r), 270.0, 180.0);
    points.extend(arc((0.0, h / 2.0 - r), (d, r), 0.0, 90.0));
    points.extend(arc((0.0, h / 2.0 + r), (d, r), 270.0, 360.0));
    points.extend(arc((2.0 * d, h - r), (d, r), 180.0, 90.0));
    points
        .into_iter()
        .map(|(px, py)| (x + px * inward, y + py))
        .collect()
}

/// Points along an ellipse from `start` to `end` degrees (0 to the right,
/// 90 below), in steps of at most 7.5 degrees.
fn arc(center: (f32, f32), radius: (f32, f32), start: f32, end: f32) -> Vec<(f32, f32)> {
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

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Form; 30] = [
        Form::Card,
        Form::LinedRect,
        Form::SmallCircle,
        Form::Stop,
        Form::Junction,
        Form::Fork,
        Form::Hourglass,
        Form::BraceLeft,
        Form::BraceRight,
        Form::Braces,
        Form::Bolt,
        Form::Document,
        Form::Delay,
        Form::LyingCylinder,
        Form::Disk,
        Form::Display,
        Form::Divided,
        Form::Triangle,
        Form::FlippedTriangle,
        Form::WindowPane,
        Form::LinedDocument,
        Form::LoopLimit,
        Form::ManualInput,
        Form::Documents,
        Form::Processes,
        Form::PaperTape,
        Form::StoredData,
        Form::Summary,
        Form::TaggedDocument,
        Form::TaggedProcess,
    ];

    fn inside(polygon: &[(f32, f32)], (px, py): (f32, f32)) -> bool {
        let mut hit = false;
        for index in 0..polygon.len() {
            let (ax, ay) = polygon[index];
            let (bx, by) = polygon[(index + 1) % polygon.len()];
            if (ay > py) != (by > py) && px < ax + (py - ay) * (bx - ax) / (by - ay) {
                hit = !hit;
            }
        }
        hit
    }

    #[test]
    fn every_form_holds_its_words() {
        // A word, a label, two lines of it.
        for text in [(20.0, 16.0), (120.0, 20.0), (90.0, 40.0)] {
            let room = (text.0 + 32.0, text.1 + 20.0);
            for form in ALL.into_iter().filter(|form| form.shows_text()) {
                let (w, h) = form.size(room, text);
                let rect = Rect {
                    x: 10.0,
                    y: 20.0,
                    w,
                    h,
                };
                let (cx, cy) = form.text_center(rect);
                let outline = if form.flat() {
                    box_of(rect)
                } else {
                    form.outline(rect)
                };
                for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    let corner = (cx + dx * text.0 / 2.0, cy + dy * text.1 / 2.0);
                    assert!(
                        inside(&outline, corner),
                        "{form:?} {text:?}: {corner:?} in {rect:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn every_name_and_alias_finds_its_form() {
        for (name, form) in [
            ("doc", Form::Document),
            ("card", Form::Card),
            ("delay", Form::Delay),
            ("bow-rect", Form::StoredData),
            ("flag", Form::PaperTape),
            ("tri", Form::Triangle),
            ("notch-rect", Form::Card),
            ("lightning-bolt", Form::Bolt),
        ] {
            assert_eq!(Form::named(name), Some(form), "{name}");
        }
        // A shape mmdr reads itself is not one of these.
        assert_eq!(Form::named("rect"), None);
        // The marks show no words, the rest do.
        assert!(!Form::Junction.shows_text() && Form::Document.shows_text());
    }

    #[test]
    fn an_outline_stays_in_its_box() {
        for form in ALL {
            let rect = Rect {
                x: 0.0,
                y: 0.0,
                w: 120.0,
                h: 60.0,
            };
            for (px, py) in form.outline(rect) {
                assert!(
                    (-0.01..=120.01).contains(&px) && (-0.01..=60.01).contains(&py),
                    "{form:?}: ({px}, {py})"
                );
            }
        }
    }
}
