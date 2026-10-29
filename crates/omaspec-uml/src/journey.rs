//! Mermaid user journeys.
//!
//! The source is read again. Tasks run in one row, left to right. A
//! section is the band over the tasks written under it. Each task carries
//! a score: the face sits higher when the score is higher, and smiles
//! above 3, frowns below 3, and is flat at 3. A score below 0 or above 6
//! keeps the face at that end, so one odd note does not stretch the page
//! or hand the diagram to another drawer. Actors are listed on the left in
//! alphabetical order, which is the order Mermaid's legend uses. A score
//! that is not a number, an extra field, a repeated actor on one task, and
//! a line this reader does not know are refused by name.

use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::{Text, keyword, text_at};
use crate::source::starts_with_ci;
use crate::svg::{self, Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const TASK_W: f32 = 150.0;
const TASK_H: f32 = 50.0;
const SECTION_H: f32 = 50.0;
const GAP: f32 = 50.0;
const FACE_R: f32 = 15.0;
const DOT_R: f32 = 7.0;
const SCORE_STEP: f32 = 30.0;
/// Section fills, then the actor dots. These are the journey palettes,
/// not the flowchart theme.
const SECTION_FILLS: [u32; 7] = [
    0x191970, 0x8B008B, 0x4B0082, 0x2F4F4F, 0x800000, 0x8B4513, 0x00008B,
];
const ACTOR_FILLS: [u32; 6] = [0x8FBC8F, 0x7CFC00, 0x00FFFF, 0x20B2AA, 0xB0E0E6, 0xFFFFE0];
const FACE: Color = Color::hex(0xFFF8DC);
const FACE_STROKE: Color = Color::hex(0x999999);
const MOUTH: Color = Color::hex(0x666666);

/// A user journey.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    /// Sections that have at least one task, in the order they were written.
    pub sections: Vec<Section>,
    /// Every actor, once, in alphabetical order.
    pub actors: Vec<String>,
    pub look: Look,
}

/// One band and the tasks under it. `label` is empty when the tasks were
/// written before any `section`.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub label: String,
    pub line: usize,
    pub tasks: Vec<Task>,
}

/// One step. `score` is the number on the line. `actors` keeps the order
/// on the line.
#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub label: String,
    pub line: usize,
    pub score: f32,
    pub actors: Vec<String>,
}

/// Type, ink, page and the two color cycles. The default cycles are the
/// classic journey palettes. A theme replaces them with its own colors.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    pub label_size: f32,
    pub title_size: f32,
    pub ink: Color,
    pub background: Paint,
    pub sections: [Color; 7],
    pub actors: [Color; 6],
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            label_size: 14.0,
            title_size: 20.0,
            ink: Color::hex(0x333333),
            background: None,
            sections: SECTION_FILLS.map(Color::hex),
            actors: ACTOR_FILLS.map(Color::hex),
        }
    }
}

/// The journey `source` describes, or `None` when it is not a `journey`
/// or it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a journey, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

/// The SVG. Labels that the text engine cannot draw decline the chart.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&diagram.look.font_name, false);
    let family = canvas.family();
    let title = title_block(diagram, family)?;
    let legend = legend_blocks(diagram, family)?;
    let legend_w = legend_width(&legend);
    let x0 = if legend_w > 0.0 { legend_w + 24.0 } else { 0.0 };
    let top = match &title {
        Some(block) => block.height + 12.0,
        None => 0.0,
    };
    let task_y = top + SECTION_H + 8.0;
    let arrow_y = task_y + TASK_H + 36.0;
    let face_base = arrow_y + 28.0 + FACE_R;
    let place = Place {
        x0,
        section_y: top,
        task_y,
        face_base,
        // Room for a face at score 0. A higher score sits further up, and
        // nothing above 6 or below 0 moves the row.
        line_bottom: face_base + 6.0 * SCORE_STEP + FACE_R + 8.0,
    };
    paint_tasks(&mut canvas, diagram, family, &place)?;
    if diagram.task_count() > 0 {
        paint_arrow(&mut canvas, diagram, x0, arrow_y);
    }
    paint_legend(&mut canvas, &legend, top);
    if let Some(block) = &title {
        canvas.text(Layer::Front, block, (x0, 0.0), Align::Left);
    }
    Ok(canvas.finish(10.0, diagram.look.background))
}

impl Diagram {
    fn task_count(&self) -> usize {
        self.sections
            .iter()
            .map(|section| section.tasks.len())
            .sum()
    }
}

fn title_block(diagram: &Diagram, family: Family) -> Result<Option<Block>, Declined> {
    let Some(title) = &diagram.title else {
        return Ok(None);
    };
    let font = Font::new(diagram.look.title_size, diagram.look.ink).bold();
    text::block(&title.value, font, family)
        .map(Some)
        .map_err(|reason| Declined::at(title.line, reason))
}

fn legend_blocks(diagram: &Diagram, family: Family) -> Result<Vec<(Color, Block)>, Declined> {
    let mut blocks = Vec::new();
    for (index, name) in diagram.actors.iter().enumerate() {
        let block = text::block(
            name,
            Font::new(diagram.look.label_size, diagram.look.ink),
            family,
        )
        .map_err(|reason| Declined::at(0, reason))?;
        blocks.push((actor_fill(diagram, index), block));
    }
    Ok(blocks)
}

fn legend_width(legend: &[(Color, Block)]) -> f32 {
    legend
        .iter()
        .map(|(_, block)| DOT_R * 2.0 + 10.0 + block.width)
        .fold(0.0, f32::max)
}

fn paint_legend(canvas: &mut Canvas, legend: &[(Color, Block)], mut y: f32) {
    for (color, block) in legend {
        let row = block.height.max(DOT_R * 2.0);
        canvas.ellipse(
            Layer::Bars,
            (DOT_R + 1.0, y + row / 2.0),
            (DOT_R, DOT_R),
            Some(*color),
            Some(Stroke::new(on_fill(*color), 1.0)),
        );
        canvas.text(
            Layer::Front,
            block,
            (DOT_R * 2.0 + 10.0, y + (row - block.height) / 2.0),
            Align::Left,
        );
        y += row + 6.0;
    }
}

struct Place {
    x0: f32,
    section_y: f32,
    task_y: f32,
    face_base: f32,
    line_bottom: f32,
}

fn paint_tasks(
    canvas: &mut Canvas,
    diagram: &Diagram,
    family: Family,
    place: &Place,
) -> Result<(), Declined> {
    let mut index = 0usize;
    for (section_index, section) in diagram.sections.iter().enumerate() {
        let count = section.tasks.len();
        if count == 0 {
            continue;
        }
        let x = place.x0 + index as f32 * (TASK_W + GAP);
        let width = count as f32 * TASK_W + (count - 1) as f32 * GAP;
        let fill = section_fill(diagram, section_index);
        canvas.rect(
            Layer::Bars,
            (x, place.section_y, width, SECTION_H),
            3.0,
            Some(fill),
            None,
        );
        if !section.label.is_empty() {
            let block = text::block(
                &section.label,
                Font::new(diagram.look.label_size, on_fill(fill)),
                family,
            )
            .map_err(|reason| Declined::at(section.line, reason))?;
            canvas.text(
                Layer::Front,
                &block,
                (
                    x + width / 2.0,
                    place.section_y + (SECTION_H - block.height) / 2.0,
                ),
                Align::Center,
            );
        }
        for task in &section.tasks {
            paint_task(canvas, diagram, family, task, fill, index, place)?;
            index += 1;
        }
    }
    Ok(())
}

fn paint_task(
    canvas: &mut Canvas,
    diagram: &Diagram,
    family: Family,
    task: &Task,
    fill: Color,
    index: usize,
    place: &Place,
) -> Result<(), Declined> {
    let x = place.x0 + index as f32 * (TASK_W + GAP);
    let center = x + TASK_W / 2.0;
    let dash = Stroke::new(MOUTH, 1.0).dashed(4.0, 2.0);
    canvas.line(
        Layer::Background,
        (center, place.task_y + TASK_H),
        (center, place.line_bottom),
        dash,
    );
    canvas.rect(
        Layer::Bars,
        (x, place.task_y, TASK_W, TASK_H),
        3.0,
        Some(fill),
        None,
    );
    let label = text::block(
        &task.label,
        Font::new(diagram.look.label_size, on_fill(fill)),
        family,
    )
    .map_err(|reason| Declined::at(task.line, reason))?;
    canvas.text(
        Layer::Front,
        &label,
        (center, place.task_y + (TASK_H - label.height) / 2.0),
        Align::Center,
    );
    for (dot, name) in task.actors.iter().enumerate() {
        let Some(actor_index) = diagram.actors.iter().position(|actor| actor == name) else {
            continue;
        };
        let color = actor_fill(diagram, actor_index);
        canvas.ellipse(
            Layer::Front,
            (x + 16.0 + dot as f32 * 18.0, place.task_y),
            (DOT_R, DOT_R),
            Some(color),
            Some(Stroke::new(on_fill(color), 1.0)),
        );
    }
    paint_face(
        canvas,
        center,
        place.face_base + (6.0 - placed_score(task.score)) * SCORE_STEP,
        task.score,
    );
    Ok(())
}

fn paint_face(canvas: &mut Canvas, cx: f32, cy: f32, score: f32) {
    canvas.ellipse(
        Layer::Bars,
        (cx, cy),
        (FACE_R, FACE_R),
        Some(FACE),
        Some(Stroke::new(FACE_STROKE, 2.0)),
    );
    let eye = Some(MOUTH);
    for side in [-1.0, 1.0] {
        canvas.ellipse(
            Layer::Front,
            (cx + side * FACE_R / 3.0, cy - FACE_R / 3.0),
            (1.5, 1.5),
            eye,
            None,
        );
    }
    if (score - 3.0).abs() < f32::EPSILON {
        canvas.line(
            Layer::Front,
            (cx - 5.0, cy + 6.0),
            (cx + 5.0, cy + 6.0),
            Stroke::new(MOUTH, 1.5),
        );
        return;
    }
    let sweep = u8::from(score > 3.0);
    let d = format!(
        "M {},{} A 6,4 0 0,{sweep} {},{}",
        svg::n(cx - 6.0),
        svg::n(cy + 3.0),
        svg::n(cx + 6.0),
        svg::n(cy + 3.0),
    );
    canvas.path(
        Layer::Front,
        &d,
        (cx - FACE_R, cy - FACE_R, cx + FACE_R, cy + FACE_R),
        None,
        Some(Stroke::new(MOUTH, 1.5)),
    );
}

fn paint_arrow(canvas: &mut Canvas, diagram: &Diagram, x0: f32, y: f32) {
    let count = diagram.task_count();
    let x1 = x0;
    let x2 = x0 + (count - 1) as f32 * (TASK_W + GAP) + TASK_W;
    let ink = diagram.look.ink;
    canvas.line(Layer::Front, (x1, y), (x2, y), Stroke::new(ink, 3.0));
    canvas.polygon(
        Layer::Front,
        &[(x2 + 12.0, y), (x2, y - 6.0), (x2, y + 6.0)],
        Some(ink),
        None,
    );
}

fn section_fill(diagram: &Diagram, index: usize) -> Color {
    diagram.look.sections[index % diagram.look.sections.len()]
}

fn actor_fill(diagram: &Diagram, index: usize) -> Color {
    diagram.look.actors[index % diagram.look.actors.len()]
}

/// Black on a light fill, white on a dark one.
fn on_fill(fill: Color) -> Color {
    if fill.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

/// Where the face sits. Scores past either end share that end's place.
fn placed_score(score: f32) -> f32 {
    score.clamp(0.0, 6.0)
}

struct Reason {
    line: usize,
    what: &'static str,
}

enum Stop {
    Foreign,
    At(Reason),
}

fn read(source: &str) -> Result<Diagram, Stop> {
    let Scanned {
        title,
        front,
        lines,
    } = scan(source);
    let Some((header, rest)) = lines.split_first() else {
        return Err(Stop::Foreign);
    };
    let opened = match header_of(header.0, &header.1) {
        Err(Stop::Foreign) => return Err(Stop::Foreign),
        other => other,
    };
    if let Some(reason) = front {
        return Err(Stop::At(reason));
    }
    opened?;
    let mut chart = Chart::new();
    if chart.title.is_none() {
        chart.title = title;
    }
    for (line, text) in rest {
        chart.line(*line, text).map_err(Stop::At)?;
    }
    Ok(chart.finish())
}

struct Scanned {
    title: Option<Text>,
    front: Option<Reason>,
    lines: Vec<(usize, String)>,
}

/// Front matter only at the top. A `---` later is a line of the journey.
fn scan(source: &str) -> Scanned {
    let mut title = None;
    let mut front = None;
    let mut lines = Vec::new();
    let mut iter = source.lines().enumerate().peekable();
    if iter.peek().is_some_and(|(_, raw)| raw.trim() == "---") {
        iter.next();
        for (no, raw) in iter.by_ref() {
            let text = raw.trim();
            if text == "---" {
                break;
            }
            if text.is_empty() || text.starts_with('#') {
                continue;
            }
            let Some((key, value)) = text.split_once(':') else {
                front.get_or_insert(Reason {
                    line: no,
                    what: "a front matter setting",
                });
                continue;
            };
            if key.trim().eq_ignore_ascii_case("title") {
                let value = unquote(value.trim());
                if !value.is_empty() {
                    title = Some(text_at(&value, no));
                }
                continue;
            }
            if !key.trim().is_empty() {
                front.get_or_insert(Reason {
                    line: no,
                    what: "a front matter setting",
                });
            }
        }
    }
    for (no, raw) in iter {
        let Some(text) = bare(raw) else { continue };
        lines.push((no, text.to_owned()));
    }
    Scanned {
        title,
        front,
        lines,
    }
}

fn bare(raw: &str) -> Option<&str> {
    let text = raw.trim();
    let text = match text.find("%%") {
        Some(at) => text[..at].trim(),
        None => text,
    };
    (!text.is_empty()).then_some(text)
}

fn unquote(text: &str) -> String {
    text.strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(text)
        .to_owned()
}

/// `journey` and nothing after it. Anything else on the header is refused.
fn header_of(line: usize, text: &str) -> Result<(), Stop> {
    match keyword(text, "journey") {
        Some("") => Ok(()),
        Some(_) => Err(Stop::At(Reason {
            line,
            what: "an unknown line",
        })),
        None => Err(Stop::Foreign),
    }
}

struct Chart {
    title: Option<Text>,
    in_descr: bool,
    sections: Vec<Section>,
}

impl Chart {
    fn new() -> Self {
        Self {
            title: None,
            in_descr: false,
            sections: Vec::new(),
        }
    }

    fn line(&mut self, line: usize, text: &str) -> Result<(), Reason> {
        if self.in_descr {
            if text.contains('}') {
                self.in_descr = false;
            }
            return Ok(());
        }
        if starts_with_ci(text, "accDescr") && text.contains('{') && !text.contains('}') {
            self.in_descr = true;
            return Ok(());
        }
        if starts_with_ci(text, "accTitle") || starts_with_ci(text, "accDescr") {
            return Ok(());
        }
        if let Some(rest) = keyword(text, "title") {
            if !rest.is_empty() {
                self.title = Some(text_at(rest, line));
            }
            return Ok(());
        }
        if let Some(rest) = keyword(text, "section") {
            self.sections.push(Section {
                label: rest.to_owned(),
                line,
                tasks: Vec::new(),
            });
            return Ok(());
        }
        self.add(task_of(line, text)?);
        Ok(())
    }

    fn add(&mut self, task: Task) {
        if self.sections.is_empty() {
            self.sections.push(Section {
                label: String::new(),
                line: task.line,
                tasks: Vec::new(),
            });
        }
        let last = self.sections.len() - 1;
        self.sections[last].tasks.push(task);
    }

    fn finish(self) -> Diagram {
        let sections: Vec<Section> = self
            .sections
            .into_iter()
            .filter(|section| !section.tasks.is_empty())
            .collect();
        let mut actors = Vec::new();
        for section in &sections {
            for task in &section.tasks {
                for actor in &task.actors {
                    if !actors.contains(actor) {
                        actors.push(actor.clone());
                    }
                }
            }
        }
        actors.sort();
        Diagram {
            title: self.title,
            sections,
            actors,
            look: Look::default(),
        }
    }
}

/// `name: score` or `name: score: actors`. The score is a plain number.
fn task_of(line: usize, text: &str) -> Result<Task, Reason> {
    if !text.contains(':') {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    }
    let mut parts = text.split(':').map(str::trim);
    let label = parts.next().unwrap_or("");
    let score = parts.next().unwrap_or("");
    let people = parts.next();
    if parts.next().is_some() {
        return Err(Reason {
            line,
            what: "an extra field",
        });
    }
    if label.is_empty() {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    }
    let score = score_of(score).map_err(|what| Reason { line, what })?;
    let actors = match people {
        Some(field) => actors_of(line, field)?,
        None => Vec::new(),
    };
    Ok(Task {
        label: label.to_owned(),
        line,
        score,
        actors,
    })
}

fn actors_of(line: usize, field: &str) -> Result<Vec<String>, Reason> {
    let mut actors = Vec::new();
    for name in field.split(',') {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if actors.iter().any(|have| have == name) {
            return Err(Reason {
                line,
                what: "a repeated actor",
            });
        }
        actors.push(name.to_owned());
    }
    Ok(actors)
}

fn score_of(raw: &str) -> Result<f32, &'static str> {
    if !plain_number(raw) {
        return Err("a score");
    }
    let value: f32 = raw.parse().map_err(|_| "a score")?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err("a score")
    }
}

fn plain_number(text: &str) -> bool {
    let rest = text.strip_prefix('-').unwrap_or(text);
    let mut parts = rest.split('.');
    let Some(whole) = parts.next() else {
        return false;
    };
    let frac = parts.next();
    if parts.next().is_some() {
        return false;
    }
    let digits = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
    digits(whole) && frac.is_none_or(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chart(source: &str) -> Diagram {
        from_mermaid(source).unwrap_or_else(|| panic!("refused {source}"))
    }

    fn refuses(source: &str, line: usize, what: &str) {
        assert_eq!(mermaid_refusal(source), Some((line, what)), "{source}");
        assert!(from_mermaid(source).is_none(), "{source}");
    }

    fn face_centers(svg: &str) -> Vec<(f32, f32)> {
        let mut centers = Vec::new();
        for piece in svg.split("<ellipse ") {
            if !piece.contains("rx=\"15\"") {
                continue;
            }
            let cx = attr(piece, "cx").parse().unwrap();
            let cy = attr(piece, "cy").parse().unwrap();
            centers.push((cx, cy));
        }
        centers
    }

    fn attr<'a>(piece: &'a str, name: &str) -> &'a str {
        let key = format!("{name}=\"");
        let rest = piece.split_once(&key).unwrap().1;
        rest.split('"').next().unwrap()
    }

    #[test]
    fn the_fixture_is_one_row_of_three_tasks() {
        let source = include_str!("../../omaspec-core/tests/fixtures/journey.mmd");
        let diagram = chart(source);
        assert_eq!(
            diagram.title.as_ref().map(|text| text.value.as_str()),
            Some("First boot")
        );
        assert_eq!(diagram.actors, ["User"]);
        assert_eq!(diagram.sections.len(), 2);
        assert_eq!(diagram.sections[0].label, "Install");
        assert_eq!(
            diagram.sections[0]
                .tasks
                .iter()
                .map(|task| (task.label.as_str(), task.score))
                .collect::<Vec<_>>(),
            [("Partition disk", 3.0), ("Pick theme", 5.0)]
        );
        assert_eq!(diagram.sections[1].label, "Use");
        assert_eq!(diagram.sections[1].tasks[0].label, "Open terminal");
        assert_eq!(diagram.sections[1].tasks[0].score, 5.0);
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("First boot")
                && svg.contains("Install")
                && svg.contains("Use")
                && svg.contains("Partition disk")
                && svg.contains("Pick theme")
                && svg.contains("Open terminal")
                && svg.contains("User")
                && svg.contains("#191970")
                && svg.contains("#8B008B")
                && svg.contains("#8FBC8F")
                && svg.contains("#FFF8DC"),
            "{svg}"
        );
        let install = svg.split("fill=\"#191970\"").nth(1).unwrap();
        let use_band = svg.split("fill=\"#8B008B\"").nth(1).unwrap();
        assert_eq!(attr(install, "y"), attr(use_band, "y"), "{svg}");
    }

    #[test]
    fn a_higher_score_sits_higher_and_actors_are_alphabetical() {
        let diagram = chart("journey\nHigh: 5: Me, Cat\nLow: 1: Me\n");
        assert_eq!(diagram.actors, ["Cat", "Me"]);
        assert_eq!(diagram.sections[0].tasks[0].actors, ["Me", "Cat"]);
        assert!(diagram.sections[0].label.is_empty());
        let svg = draw(&diagram).unwrap();
        let mut faces = face_centers(&svg);
        faces.sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap());
        assert_eq!(faces.len(), 2, "{svg}");
        assert!((faces[1].1 - faces[0].1 - 120.0).abs() < 0.05, "{faces:?}");
    }

    #[test]
    fn a_score_past_the_ends_stays_on_this_row() {
        let diagram = chart(
            "journey\n\
             High: 6: Me\n\
             Mid: 5: Me\n\
             Low: 0: Me\n\
             Neg: -1: Me\n\
             Far: 40: Me\n",
        );
        assert_eq!(
            diagram.sections[0]
                .tasks
                .iter()
                .map(|task| task.score)
                .collect::<Vec<_>>(),
            [6.0, 5.0, 0.0, -1.0, 40.0]
        );
        let svg = draw(&diagram).unwrap();
        let mut faces = face_centers(&svg);
        faces.sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap());
        assert_eq!(faces.len(), 5, "{svg}");
        let y = |index: usize| faces[index].1;
        assert!(
            (y(0) - y(4)).abs() < 0.05,
            "6 and 40 share the top {faces:?}"
        );
        assert!(
            (y(2) - y(3)).abs() < 0.05,
            "0 and -1 share the bottom {faces:?}"
        );
        assert!((y(1) - y(0) - SCORE_STEP).abs() < 0.05, "{faces:?}");
        assert!((y(2) - y(0) - 6.0 * SCORE_STEP).abs() < 0.05, "{faces:?}");
        let task_y: Vec<&str> = svg
            .split("<rect ")
            .skip(1)
            .filter(|piece| attr(piece, "width") == "150" && attr(piece, "height") == "50")
            .map(|piece| attr(piece, "y"))
            .collect();
        assert!(
            task_y.len() == 5 && task_y.windows(2).all(|pair| pair[0] == pair[1]),
            "{task_y:?}"
        );
    }

    #[test]
    fn the_last_title_wins_and_notes_are_skipped() {
        let diagram = chart(
            "---\n\
             title: Front\n\
             ---\n\
             journey\n\
             title Header\n\
             accTitle: hidden\n\
             accDescr {\n\
               several lines\n\
             }\n\
             %% a note\n\
             title Body\n\
             section Go\n\
             Work: 4: Me\n",
        );
        assert_eq!(
            diagram.title.as_ref().map(|text| text.value.as_str()),
            Some("Body")
        );
        assert_eq!(diagram.sections[0].tasks[0].score, 4.0);
        let front = chart("---\ntitle: Front\n---\njourney\nWork: 2: Me\n");
        assert_eq!(
            front.title.as_ref().map(|text| text.value.as_str()),
            Some("Front")
        );
        assert!(mermaid_refusal("---\ntitle: Hi\n---\nsequenceDiagram\nA->>B: hi\n").is_none());
        let empty = draw(&chart("journey\ntitle Waiting\n")).unwrap();
        assert!(empty.contains("Waiting"), "{empty}");
        assert!(!empty.contains("rx=\"15\""), "{empty}");
    }

    #[test]
    fn refuses_a_step_it_cannot_place() {
        refuses("journey foo\n", 0, "an unknown line");
        refuses("journey\nclick Me\n", 1, "an unknown line");
        refuses("journey\n: 5: Me\n", 1, "an unknown line");
        refuses("journey\nNap: nope: Me\n", 1, "a score");
        refuses("journey\nNap: 5: Me: extra\n", 1, "an extra field");
        refuses("journey\nNap: 5: Me, Me\n", 1, "a repeated actor");
        refuses(
            "---\ndisplayMode: compact\n---\njourney\n",
            1,
            "a front matter setting",
        );
    }
}
