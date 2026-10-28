//! Mermaid timelines.
//!
//! The source is read again. Time periods run in one row, left to right.
//! `timeline TD` uses the same boxes down the page, with the events to the
//! right, so that direction does not hand the diagram to another drawer.
//! A section is the band over the periods written under it. Events sit on
//! the cross axis. Without a named section, each period takes the next
//! color.

use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::{Text, keyword, text_at};
use crate::source::starts_with_ci;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const BOX_W: f32 = 150.0;
const GAP: f32 = 50.0;
const STEP: f32 = BOX_W + GAP;
const SECTION_H: f32 = 44.0;
const PERIOD_H: f32 = 50.0;
const EVENT_H: f32 = 40.0;
const EVENT_GAP: f32 = 10.0;
/// A vertical timeline keeps the arrow clear of the period column.
const DOWN_X: f32 = 28.0;
const ARROW_X: f32 = 8.0;
const BESIDE: f32 = 24.0;
const EVENT_STEP: f32 = BOX_W + 16.0;
const ROW: f32 = PERIOD_H + GAP;
/// The default timeline scale: Mermaid's `cScale`, darkened once. A theme
/// replaces it.
const FILLS: [u32; 12] = [
    0xB9B9FF, 0xFFFFAB, 0xE9FFB9, 0xDEB9FF, 0xFFB9FF, 0xFFB9DE, 0xFFB9B9, 0xFFDEB9, 0xDEFFB9,
    0xB9FFDA, 0xB9FFFF, 0xB9DEFF,
];

/// A timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    /// Sections that have at least one period, in the order they were written.
    pub sections: Vec<Section>,
    /// `timeline TD`: periods run down the page.
    pub down: bool,
    pub look: Look,
}

/// One band and the periods under it. `label` is empty when the periods
/// were written before any `section`.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub label: String,
    pub line: usize,
    pub periods: Vec<Period>,
}

/// One time period. `events` keeps the order they were written.
#[derive(Debug, Clone, PartialEq)]
pub struct Period {
    pub label: String,
    pub line: usize,
    pub events: Vec<Event>,
}

/// One event under a period.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub label: String,
    pub line: usize,
}

/// Type, ink, page and the section colors. The default colors are the
/// timeline scale. A theme replaces them.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    pub label_size: f32,
    pub title_size: f32,
    pub ink: Color,
    pub background: Paint,
    pub sections: [Color; 12],
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            label_size: 14.0,
            title_size: 20.0,
            ink: Color::hex(0x333333),
            background: None,
            sections: FILLS.map(Color::hex),
        }
    }
}

/// The timeline `source` describes, or `None` when it is not a `timeline`
/// or it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a timeline, and one it accepts, are `None`.
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
    let top = match &title {
        Some(block) => block.height + 12.0,
        None => 0.0,
    };
    if diagram.down {
        paint_down(&mut canvas, diagram, family, top)?;
    } else {
        paint_across(&mut canvas, diagram, family, top)?;
    }
    if let Some(block) = &title {
        let (x, align) = title_anchor(diagram);
        canvas.text(Layer::Front, block, (x, 0.0), align);
    }
    Ok(canvas.finish(10.0, diagram.look.background))
}

fn title_anchor(diagram: &Diagram) -> (f32, Align) {
    if diagram.period_count() == 0 {
        return (0.0, Align::Left);
    }
    if diagram.down {
        let width = diagram.sections.iter().map(row_width).fold(0.0, f32::max);
        return (DOWN_X + width / 2.0, Align::Center);
    }
    let count = diagram.period_count();
    let width = (count - 1) as f32 * STEP + BOX_W;
    (width / 2.0, Align::Center)
}

fn paint_across(
    canvas: &mut Canvas,
    diagram: &Diagram,
    family: Family,
    top: f32,
) -> Result<(), Declined> {
    let bands = diagram
        .sections
        .iter()
        .any(|section| !section.label.is_empty());
    let period_y = if bands { top + SECTION_H + 16.0 } else { top };
    let event_y = period_y + PERIOD_H + 28.0;
    let depth = event_depth(diagram);
    let line_bottom = if depth > 0.0 {
        event_y + depth + 12.0
    } else {
        event_y
    };
    let arrow_y = if depth > 0.0 {
        line_bottom + 28.0
    } else {
        period_y + PERIOD_H + 36.0
    };
    let place = Place {
        section_y: top,
        period_y,
        event_y,
        line_bottom,
        arrow_y,
        bands,
    };
    paint(canvas, diagram, family, &place)?;
    let count = diagram.period_count();
    if count > 0 {
        let width = (count - 1) as f32 * STEP + BOX_W;
        paint_arrow(canvas, width, place.arrow_y, diagram.look.ink);
    }
    Ok(())
}

impl Diagram {
    fn period_count(&self) -> usize {
        self.sections
            .iter()
            .map(|section| section.periods.len())
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

fn event_depth(diagram: &Diagram) -> f32 {
    let slots = diagram
        .sections
        .iter()
        .flat_map(|section| section.periods.iter())
        .map(|period| period.events.len())
        .max()
        .unwrap_or(0);
    if slots == 0 {
        0.0
    } else {
        slots as f32 * EVENT_H + (slots - 1) as f32 * EVENT_GAP
    }
}

struct Place {
    section_y: f32,
    period_y: f32,
    event_y: f32,
    line_bottom: f32,
    arrow_y: f32,
    bands: bool,
}

fn paint(
    canvas: &mut Canvas,
    diagram: &Diagram,
    family: Family,
    place: &Place,
) -> Result<(), Declined> {
    let mut index = 0usize;
    for (section_index, section) in diagram.sections.iter().enumerate() {
        let count = section.periods.len();
        if count == 0 {
            continue;
        }
        let x = index as f32 * STEP;
        let width = count as f32 * BOX_W + (count - 1) as f32 * GAP;
        if place.bands {
            let fill = fill_of(diagram, section_index);
            canvas.rect(
                Layer::Bars,
                (x, place.section_y, width, SECTION_H),
                5.0,
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
        }
        for period in &section.periods {
            let fill = fill_of(diagram, if place.bands { section_index } else { index });
            paint_period(canvas, diagram, family, period, fill, index, place)?;
            index += 1;
        }
    }
    Ok(())
}

fn paint_period(
    canvas: &mut Canvas,
    diagram: &Diagram,
    family: Family,
    period: &Period,
    fill: Color,
    index: usize,
    place: &Place,
) -> Result<(), Declined> {
    let x = index as f32 * STEP;
    let center = x + BOX_W / 2.0;
    if !period.events.is_empty() {
        let dash = Stroke::new(diagram.look.ink, 2.0).dashed(5.0, 5.0);
        canvas.line(
            Layer::Background,
            (center, place.period_y + PERIOD_H),
            (center, place.line_bottom),
            dash,
        );
    }
    canvas.rect(
        Layer::Bars,
        (x, place.period_y, BOX_W, PERIOD_H),
        5.0,
        Some(fill),
        None,
    );
    let label = text::block(
        &period.label,
        Font::new(diagram.look.label_size, on_fill(fill)),
        family,
    )
    .map_err(|reason| Declined::at(period.line, reason))?;
    canvas.text(
        Layer::Front,
        &label,
        (center, place.period_y + (PERIOD_H - label.height) / 2.0),
        Align::Center,
    );
    let card = event_fill(fill);
    let mut y = place.event_y;
    for event in &period.events {
        canvas.rect(Layer::Bars, (x, y, BOX_W, EVENT_H), 5.0, Some(card), None);
        let block = text::block(
            &event.label,
            Font::new(diagram.look.label_size, on_fill(card)),
            family,
        )
        .map_err(|reason| Declined::at(event.line, reason))?;
        canvas.text(
            Layer::Front,
            &block,
            (center, y + (EVENT_H - block.height) / 2.0),
            Align::Center,
        );
        y += EVENT_H + EVENT_GAP;
    }
    Ok(())
}

fn paint_arrow(canvas: &mut Canvas, width: f32, y: f32, ink: Color) {
    canvas.line(Layer::Front, (0.0, y), (width, y), Stroke::new(ink, 4.0));
    canvas.polygon(
        Layer::Front,
        &[(width + 12.0, y), (width, y - 6.0), (width, y + 6.0)],
        Some(ink),
        None,
    );
}

fn paint_arrow_down(canvas: &mut Canvas, x: f32, y0: f32, y1: f32, ink: Color) {
    canvas.line(Layer::Front, (x, y0), (x, y1), Stroke::new(ink, 4.0));
    canvas.polygon(
        Layer::Front,
        &[(x, y1 + 12.0), (x - 6.0, y1), (x + 6.0, y1)],
        Some(ink),
        None,
    );
}

/// Periods in one column. The section band sits over its rows. Events sit
/// to the right of their period, on the same row.
fn paint_down(
    canvas: &mut Canvas,
    diagram: &Diagram,
    family: Family,
    top: f32,
) -> Result<(), Declined> {
    let bands = diagram
        .sections
        .iter()
        .any(|section| !section.label.is_empty());
    let mut y = top;
    let mut index = 0usize;
    let mut last_bottom = top;
    for (section_index, section) in diagram.sections.iter().enumerate() {
        if section.periods.is_empty() {
            continue;
        }
        if bands {
            let fill = fill_of(diagram, section_index);
            let width = row_width(section);
            canvas.rect(
                Layer::Bars,
                (DOWN_X, y, width, SECTION_H),
                5.0,
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
                    (DOWN_X + width / 2.0, y + (SECTION_H - block.height) / 2.0),
                    Align::Center,
                );
            }
            y += SECTION_H + 16.0;
        }
        for period in &section.periods {
            let fill = fill_of(diagram, if bands { section_index } else { index });
            paint_period_down(canvas, diagram, family, period, fill, y)?;
            last_bottom = y + PERIOD_H;
            y += ROW;
            index += 1;
        }
    }
    if index > 0 {
        paint_arrow_down(canvas, ARROW_X, top, last_bottom, diagram.look.ink);
    }
    Ok(())
}

fn paint_period_down(
    canvas: &mut Canvas,
    diagram: &Diagram,
    family: Family,
    period: &Period,
    fill: Color,
    y: f32,
) -> Result<(), Declined> {
    let x = DOWN_X;
    canvas.rect(Layer::Bars, (x, y, BOX_W, PERIOD_H), 5.0, Some(fill), None);
    let label = text::block(
        &period.label,
        Font::new(diagram.look.label_size, on_fill(fill)),
        family,
    )
    .map_err(|reason| Declined::at(period.line, reason))?;
    canvas.text(
        Layer::Front,
        &label,
        (x + BOX_W / 2.0, y + (PERIOD_H - label.height) / 2.0),
        Align::Center,
    );
    if period.events.is_empty() {
        return Ok(());
    }
    let mid = y + PERIOD_H / 2.0;
    let far = x + BOX_W + BESIDE + (period.events.len() - 1) as f32 * EVENT_STEP + BOX_W;
    let dash = Stroke::new(diagram.look.ink, 2.0).dashed(5.0, 5.0);
    canvas.line(Layer::Background, (x + BOX_W, mid), (far, mid), dash);
    let card = event_fill(fill);
    let event_y = y + (PERIOD_H - EVENT_H) / 2.0;
    for (slot, event) in period.events.iter().enumerate() {
        let ex = x + BOX_W + BESIDE + slot as f32 * EVENT_STEP;
        canvas.rect(
            Layer::Bars,
            (ex, event_y, BOX_W, EVENT_H),
            5.0,
            Some(card),
            None,
        );
        let block = text::block(
            &event.label,
            Font::new(diagram.look.label_size, on_fill(card)),
            family,
        )
        .map_err(|reason| Declined::at(event.line, reason))?;
        canvas.text(
            Layer::Front,
            &block,
            (ex + BOX_W / 2.0, event_y + (EVENT_H - block.height) / 2.0),
            Align::Center,
        );
    }
    Ok(())
}

fn row_width(section: &Section) -> f32 {
    let events = section
        .periods
        .iter()
        .map(|period| period.events.len())
        .max()
        .unwrap_or(0);
    if events == 0 {
        BOX_W
    } else {
        BOX_W + BESIDE + events as f32 * BOX_W + (events - 1) as f32 * 16.0
    }
}

fn fill_of(diagram: &Diagram, index: usize) -> Color {
    diagram.look.sections[index % diagram.look.sections.len()]
}

/// Black on a light fill, white on a dark one.
fn on_fill(fill: Color) -> Color {
    if fill.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

/// The event card stays in the period's color and a step away from it.
/// A light scale lightens, which is the classic timeline. A dark theme
/// lightens further, so the card does not disappear into the period.
fn event_fill(fill: Color) -> Color {
    if fill.is_dark() {
        fill.mix(Color::WHITE, 0.45)
    } else {
        fill.mix(Color::WHITE, 0.2)
    }
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
    let opened = match header_of(&header.1) {
        Err(Stop::Foreign) => return Err(Stop::Foreign),
        other => other,
    };
    if let Some(reason) = front {
        return Err(Stop::At(reason));
    }
    let Opened { down, on_header } = opened?;
    let mut chart = Chart::new();
    chart.down = down;
    if chart.title.is_none() {
        chart.title = title;
    }
    if let Some(text) = on_header {
        chart.line(header.0, &text).map_err(Stop::At)?;
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

/// Front matter only at the top. A `---` later is a line of the timeline.
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
        lines.push((no, text));
    }
    Scanned {
        title,
        front,
        lines,
    }
}

fn bare(raw: &str) -> Option<String> {
    let text = raw.trim();
    let text = match text.find("%%") {
        Some(at) => text[..at].trim(),
        None => text,
    };
    if text.is_empty() || text.starts_with('#') {
        None
    } else {
        Some(text.to_owned())
    }
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

struct Opened {
    down: bool,
    on_header: Option<String>,
}

/// `timeline`, `timeline LR`, `timeline TD`, or a first period on the header.
fn header_of(text: &str) -> Result<Opened, Stop> {
    let Some(rest) = keyword(text, "timeline") else {
        return Err(Stop::Foreign);
    };
    if rest.is_empty() || keyword(rest, "LR").is_some_and(|more| more.is_empty()) {
        return Ok(Opened {
            down: false,
            on_header: None,
        });
    }
    if keyword(rest, "TD").is_some_and(|more| more.is_empty()) {
        return Ok(Opened {
            down: true,
            on_header: None,
        });
    }
    Ok(Opened {
        down: false,
        on_header: Some(rest.to_owned()),
    })
}

struct Chart {
    title: Option<Text>,
    down: bool,
    in_descr: bool,
    sections: Vec<Section>,
}

impl Chart {
    fn new() -> Self {
        Self {
            title: None,
            down: false,
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
        if let Some(rest) = keyword(text, "title")
            && !rest.is_empty()
        {
            self.title = Some(text_at(&breaks(rest), line));
            return Ok(());
        }
        if let Some(rest) = keyword(text, "section")
            && !rest.is_empty()
        {
            let (label, events) = split_section(rest);
            self.sections.push(Section {
                label,
                line,
                periods: Vec::new(),
            });
            return self.add_events(line, events);
        }
        let text = cut_hash_comment(text);
        if text.is_empty() {
            return Ok(());
        }
        if let Some(rest) = text.strip_prefix(':') {
            return self.add_events(line, events_of(rest));
        }
        if let Some((period, rest)) = text.split_once(':') {
            let label = period.trim();
            if label.is_empty() {
                return self.add_events(line, events_of(rest));
            }
            self.add_period(Period {
                label: breaks(label),
                line,
                events: events_of(rest)
                    .into_iter()
                    .map(|label| Event { label, line })
                    .collect(),
            });
            return Ok(());
        }
        self.add_period(Period {
            label: breaks(text),
            line,
            events: Vec::new(),
        });
        Ok(())
    }

    fn add_period(&mut self, period: Period) {
        if self.sections.is_empty() {
            self.sections.push(Section {
                label: String::new(),
                line: period.line,
                periods: Vec::new(),
            });
        }
        let last = self.sections.len() - 1;
        self.sections[last].periods.push(period);
    }

    fn add_events(&mut self, line: usize, events: Vec<String>) -> Result<(), Reason> {
        if events.is_empty() {
            return Ok(());
        }
        let Some(period) = self
            .sections
            .last_mut()
            .and_then(|section| section.periods.last_mut())
        else {
            return Err(Reason {
                line,
                what: "an event",
            });
        };
        for label in events {
            period.events.push(Event { label, line });
        }
        Ok(())
    }

    fn finish(self) -> Diagram {
        let sections = self
            .sections
            .into_iter()
            .filter(|section| !section.periods.is_empty())
            .collect();
        Diagram {
            title: self.title,
            sections,
            down: self.down,
            look: Look::default(),
        }
    }
}

fn split_section(rest: &str) -> (String, Vec<String>) {
    match rest.split_once(':') {
        Some((label, events)) => (breaks(label.trim()), events_of(events)),
        None => (breaks(rest), Vec::new()),
    }
}

fn events_of(text: &str) -> Vec<String> {
    text.split(':')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(breaks)
        .collect()
}

/// A ` #` before any colon ends the line, the way a `#` comment does.
fn cut_hash_comment(text: &str) -> &str {
    match text.find(" #") {
        Some(at) if !text[..at].contains(':') => text[..at].trim(),
        _ => text,
    }
}

/// `<br>` becomes a line break. The letters around it stay.
fn breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = find_br(rest) {
        out.push_str(rest[..at].trim_end());
        out.push('\n');
        let tail = &rest[at + 3..];
        let end = tail.find('>').unwrap_or(0);
        rest = tail[end + 1..].trim_start();
    }
    out.push_str(rest);
    out
}

fn find_br(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    let at = lower.find("<br")?;
    let tail = &lower[at + 3..];
    let end = tail.find('>')?;
    let inside = &tail[..end];
    if inside.chars().all(|c| c.is_ascii_whitespace() || c == '/') {
        Some(at)
    } else {
        None
    }
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

    fn attr<'a>(piece: &'a str, name: &str) -> &'a str {
        let key = format!("{name}=\"");
        let rest = piece.split_once(&key).unwrap().1;
        rest.split('"').next().unwrap()
    }

    fn rects<'a>(svg: &'a str, fill: &str) -> Vec<&'a str> {
        svg.split("<rect ")
            .filter(|piece| piece.contains(&format!("fill=\"{fill}\"")))
            .collect()
    }

    #[test]
    fn sections_share_one_row_and_events_stack() {
        let diagram = chart(
            "timeline\n\
             title History of Social Media Platform\n\
             section Early\n\
             2002 : LinkedIn\n\
             2004 : Facebook : Google\n\
             section Later\n\
             2006 : Twitter\n\
                  : Vine\n",
        );
        assert_eq!(
            diagram.title.as_ref().map(|text| text.value.as_str()),
            Some("History of Social Media Platform")
        );
        assert_eq!(diagram.sections.len(), 2);
        assert_eq!(diagram.sections[0].label, "Early");
        assert_eq!(diagram.sections[0].periods[0].label, "2002");
        assert_eq!(
            diagram.sections[0].periods[0]
                .events
                .iter()
                .map(|event| event.label.as_str())
                .collect::<Vec<_>>(),
            ["LinkedIn"]
        );
        assert_eq!(
            diagram.sections[0].periods[1]
                .events
                .iter()
                .map(|event| event.label.as_str())
                .collect::<Vec<_>>(),
            ["Facebook", "Google"]
        );
        assert_eq!(diagram.sections[1].periods[0].label, "2006");
        let svg = draw(&diagram).unwrap();
        for word in [
            "History of Social Media Platform",
            "Early",
            "Later",
            "2002",
            "LinkedIn",
            "Facebook",
            "Google",
            "Twitter",
            "Vine",
        ] {
            assert!(svg.contains(word), "{svg} missing {word}");
        }
        assert!(svg.contains("#B9B9FF") && svg.contains("#FFFFAB"), "{svg}");
        assert!(svg.contains("stroke-dasharray"), "{svg}");
        let early = rects(&svg, "#B9B9FF");
        let later = rects(&svg, "#FFFFAB");
        assert!(early.len() >= 2 && later.len() >= 2, "{svg}");
        assert_eq!(attr(early[0], "y"), attr(later[0], "y"), "{svg}");
        let periods: Vec<_> = early
            .iter()
            .chain(later.iter())
            .filter(|piece| attr(piece, "height") == "50")
            .collect();
        assert!(periods.len() >= 2, "{svg}");
        let y = attr(periods[0], "y");
        assert!(periods.iter().all(|piece| attr(piece, "y") == y), "{svg}");
    }

    #[test]
    fn without_a_section_each_period_takes_the_next_color() {
        let diagram = chart(
            "timeline\n\
             2002 : LinkedIn\n\
                  : Extra\n\
             2004 : Facebook : Google\n",
        );
        assert!(diagram.sections[0].label.is_empty());
        assert_eq!(diagram.sections[0].periods.len(), 2);
        assert_eq!(
            diagram.sections[0].periods[0]
                .events
                .iter()
                .map(|event| event.label.as_str())
                .collect::<Vec<_>>(),
            ["LinkedIn", "Extra"]
        );
        let svg = draw(&diagram).unwrap();
        let first = rects(&svg, "#B9B9FF");
        let second = rects(&svg, "#FFFFAB");
        let first_period = first
            .iter()
            .find(|piece| attr(piece, "height") == "50")
            .unwrap();
        let second_period = second
            .iter()
            .find(|piece| attr(piece, "height") == "50")
            .unwrap();
        assert_eq!(attr(first_period, "y"), attr(second_period, "y"), "{svg}");
        assert!(svg.contains("<polygon"), "{svg}");
    }

    #[test]
    fn the_last_title_wins_and_a_break_splits_the_line() {
        let diagram = chart(
            "---\n\
             title: Front\n\
             ---\n\
             timeline\n\
             title Header\n\
             accTitle: hidden\n\
             accDescr {\n\
               several lines\n\
             }\n\
             %% a note\n\
             # a note\n\
             title Body\n\
             section Go\n\
             Steam <br>power : Engine\n\
             2002 # dropped\n",
        );
        assert_eq!(
            diagram.title.as_ref().map(|text| text.value.as_str()),
            Some("Body")
        );
        assert_eq!(diagram.sections[0].periods[0].label, "Steam\npower");
        assert_eq!(diagram.sections[0].periods[1].label, "2002");
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("Steam") && svg.contains("power") && svg.contains("Engine"),
            "{svg}"
        );
        let front = chart("---\ntitle: Front\n---\ntimeline\n2002 : A\n");
        assert_eq!(
            front.title.as_ref().map(|text| text.value.as_str()),
            Some("Front")
        );
        assert!(mermaid_refusal("---\ntitle: Hi\n---\nsequenceDiagram\nA->>B: hi\n").is_none());
        let sideways = chart("timeline LR\n2002 : Launch\n");
        assert!(!sideways.down);
        assert_eq!(sideways.sections[0].periods[0].events[0].label, "Launch");
        let waiting = draw(&chart("timeline\ntitle Waiting\n")).unwrap();
        assert!(waiting.contains("Waiting"), "{waiting}");
        assert!(!waiting.contains("<rect "), "{waiting}");
    }

    #[test]
    fn a_vertical_timeline_is_one_column() {
        let diagram = chart(
            "timeline TD\n\
             title History\n\
             section Early\n\
             2002 : LinkedIn\n\
             2004 : Facebook : Google\n\
             section Later\n\
             2006 : Twitter\n",
        );
        assert!(diagram.down);
        let svg = draw(&diagram).unwrap();
        for word in ["History", "Early", "Later", "LinkedIn", "Google", "Twitter"] {
            assert!(svg.contains(word), "{svg} missing {word}");
        }
        let mut periods: Vec<(f32, f32)> = svg
            .split("<rect ")
            .skip(1)
            .filter(|piece| attr(piece, "width") == "150" && attr(piece, "height") == "50")
            .map(|piece| {
                (
                    attr(piece, "x").parse().unwrap(),
                    attr(piece, "y").parse().unwrap(),
                )
            })
            .collect();
        periods.sort_by(|left, right| left.1.partial_cmp(&right.1).unwrap());
        assert_eq!(periods.len(), 3, "{svg}");
        assert!(
            periods.iter().all(|period| period.0 == periods[0].0),
            "{periods:?}"
        );
        assert!(
            (periods[1].1 - periods[0].1 - ROW).abs() < 0.05,
            "{periods:?}"
        );
        let mut events: Vec<(f32, f32)> = svg
            .split("<rect ")
            .skip(1)
            .filter(|piece| attr(piece, "height") == "40")
            .map(|piece| {
                (
                    attr(piece, "x").parse().unwrap(),
                    attr(piece, "y").parse().unwrap(),
                )
            })
            .collect();
        events.sort_by(|left, right| {
            left.1
                .partial_cmp(&right.1)
                .unwrap()
                .then(left.0.partial_cmp(&right.0).unwrap())
        });
        assert_eq!(events.len(), 4, "{svg}");
        assert!(
            (events[1].1 - events[2].1).abs() < 0.05 && events[2].0 > events[1].0,
            "the two events of one period share its row {events:?}"
        );
        assert!(events[1].0 > periods[0].0, "{events:?} {periods:?}");
        assert!(events[0].1 < events[1].1 && events[2].1 < events[3].1);
        let waiting = draw(&chart("timeline TD\ntitle Waiting\n")).unwrap();
        assert!(
            waiting.contains("Waiting") && !waiting.contains("<rect "),
            "{waiting}"
        );
        assert!(!chart("timeline\n2002 : A\n").down);
    }

    #[test]
    fn refuses_a_step_it_cannot_place() {
        refuses("timeline\n: Launch\n", 1, "an event");
        refuses(
            "---\ndisplayMode: compact\n---\ntimeline\n",
            1,
            "a front matter setting",
        );
    }
}
