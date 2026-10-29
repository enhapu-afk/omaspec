//! Mermaid pie charts.
//!
//! The source is read again. A slice is `label : number`, and the number
//! is kept as it was written so `showData` can print it unchanged. Wedges
//! start at the top and run clockwise. One slice that is the whole chart
//! is an ellipse: an arc that ends where it starts draws nothing. A
//! negative number stays in the legend and takes no wedge, so one minus
//! does not hand the chart to another drawer. A value that is not a plain
//! number, a repeated label and a line this reader does not know are
//! refused by name.

use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::{Text, keyword, text_at};
use crate::source::starts_with_ci;
use crate::svg::{self, Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

/// Radius of the pie. The same circle is the path's bounds.
const RADIUS: f32 = 88.0;
const TITLE_GAP: f32 = 12.0;
const LEGEND_GAP: f32 = 28.0;
const SWATCH: f32 = 14.0;
const ROW_GAP: f32 = 6.0;
/// A turn this close to a full circle is drawn as an ellipse.
const ALMOST_TURN: f32 = std::f32::consts::TAU - 0.001;
/// A wedge thinner than this would be a speck, so it stays in the legend.
const DUST: f32 = 0.0001;

/// A pie chart.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    pub show_data: bool,
    pub slices: Vec<Slice>,
    pub look: Look,
}

/// One slice, in the order it was written. A negative `value` draws no
/// wedge. `written` is the number text, so `1.5` is not reprinted as a float.
#[derive(Debug, Clone, PartialEq)]
pub struct Slice {
    pub label: String,
    pub value: f32,
    pub written: String,
    pub line: usize,
    pub color: Option<Color>,
}

/// How the chart is painted. The reader leaves this opaque and uncolored;
/// the Mermaid theme fills it in before [`draw`].
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    pub label_size: f32,
    pub title_size: f32,
    /// Legend text.
    pub ink: Color,
    pub title_ink: Color,
    pub background: Paint,
    /// The line between two wedges.
    pub fill_stroke: Color,
    /// The circle around the pie.
    pub ring: Color,
    pub stroke_width: f32,
    pub ring_width: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            label_size: 14.0,
            title_size: 16.0,
            ink: Color::BLACK,
            title_ink: Color::BLACK,
            background: None,
            fill_stroke: Color::BLACK,
            ring: Color::BLACK,
            stroke_width: 1.0,
            ring_width: 1.0,
        }
    }
}

/// The pie chart `source` describes, or `None` when it is not a `pie` or
/// it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a pie chart, and one it accepts, are `None`.
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
        Some(block) => block.height + TITLE_GAP,
        None => 0.0,
    };
    let cx = RADIUS;
    let cy = top + RADIUS;
    paint_slices(&mut canvas, diagram, cx, cy);
    paint_ring(&mut canvas, &diagram.look, cx, cy);
    if let Some(block) = &title {
        canvas.text(Layer::Front, block, (cx, 0.0), Align::Center);
    }
    let legend_x = legend_x(cx, title.as_ref());
    paint_legend(&mut canvas, diagram, family, legend_x, cy - RADIUS)?;
    Ok(canvas.finish(10.0, diagram.look.background))
}

fn title_block(diagram: &Diagram, family: Family) -> Result<Option<Block>, Declined> {
    let Some(title) = &diagram.title else {
        return Ok(None);
    };
    let font = Font::new(diagram.look.title_size, diagram.look.title_ink).bold();
    text::block(&title.value, font, family)
        .map(Some)
        .map_err(|reason| Declined::at(title.line, reason))
}

fn paint_slices(canvas: &mut Canvas, diagram: &Diagram, cx: f32, cy: f32) {
    let total: f32 = diagram
        .slices
        .iter()
        .map(|slice| slice.value.max(0.0))
        .sum();
    if total <= 0.0 {
        return;
    }
    let mut theta = 0.0;
    for (index, slice) in diagram.slices.iter().enumerate() {
        if slice.value <= 0.0 {
            continue;
        }
        let span = slice.value / total * std::f32::consts::TAU;
        if !span.is_finite() {
            continue;
        }
        if span <= DUST {
            theta += span;
            continue;
        }
        let fill = Some(fill_of(slice, index));
        if span >= ALMOST_TURN {
            canvas.ellipse(Layer::Bars, (cx, cy), (RADIUS, RADIUS), fill, None);
        } else {
            let stroke = (diagram.look.stroke_width > 0.0)
                .then(|| Stroke::new(diagram.look.fill_stroke, diagram.look.stroke_width));
            canvas.path(
                Layer::Bars,
                &wedge(cx, cy, theta, theta + span),
                (cx - RADIUS, cy - RADIUS, cx + RADIUS, cy + RADIUS),
                fill,
                stroke,
            );
        }
        theta += span;
    }
}

/// Clockwise from the top: `x = cx + r sin θ`, `y = cy − r cos θ`.
fn wedge(cx: f32, cy: f32, start: f32, end: f32) -> String {
    let (x0, y0) = point(cx, cy, start);
    let (x1, y1) = point(cx, cy, end);
    let large = u8::from(end - start > std::f32::consts::PI);
    format!(
        "M {},{} L {},{} A {},{} 0 {large},1 {},{} Z",
        svg::n(cx),
        svg::n(cy),
        svg::n(x0),
        svg::n(y0),
        svg::n(RADIUS),
        svg::n(RADIUS),
        svg::n(x1),
        svg::n(y1),
    )
}

fn point(cx: f32, cy: f32, theta: f32) -> (f32, f32) {
    (cx + RADIUS * theta.sin(), cy - RADIUS * theta.cos())
}

fn paint_ring(canvas: &mut Canvas, look: &Look, cx: f32, cy: f32) {
    if look.ring_width <= 0.0 {
        return;
    }
    canvas.ellipse(
        Layer::Bars,
        (cx, cy),
        (RADIUS, RADIUS),
        None,
        Some(Stroke::new(look.ring, look.ring_width)),
    );
}

fn legend_x(cx: f32, title: Option<&Block>) -> f32 {
    let circle_right = cx + RADIUS;
    let title_right = title
        .map(|block| cx + block.width / 2.0)
        .unwrap_or(circle_right);
    circle_right.max(title_right) + LEGEND_GAP
}

fn paint_legend(
    canvas: &mut Canvas,
    diagram: &Diagram,
    family: Family,
    x: f32,
    mut y: f32,
) -> Result<(), Declined> {
    for (index, slice) in diagram.slices.iter().enumerate() {
        let label = if diagram.show_data {
            format!("{} [{}]", slice.label, slice.written)
        } else {
            slice.label.clone()
        };
        let block = text::block(
            &label,
            Font::new(diagram.look.label_size, diagram.look.ink),
            family,
        )
        .map_err(|reason| Declined::at(slice.line, reason))?;
        let row = block.height.max(SWATCH);
        canvas.rect(
            Layer::Bars,
            (x, y + (row - SWATCH) / 2.0, SWATCH, SWATCH),
            2.0,
            Some(fill_of(slice, index)),
            None,
        );
        canvas.text(
            Layer::Front,
            &block,
            (x + SWATCH + 8.0, y + (row - block.height) / 2.0),
            Align::Left,
        );
        y += row + ROW_GAP;
    }
    Ok(())
}

fn fill_of(slice: &Slice, index: usize) -> Color {
    /// Used only when the theme has not painted the slice.
    const FALLBACK: [u32; 12] = [
        0x7B88A8, 0xE8C547, 0x6AA84F, 0xE06666, 0x6FA8DC, 0x8E7CC3, 0xF6B26B, 0x76A5AF, 0xC27BA0,
        0xA4C2F4, 0xB6D7A8, 0xD5A6BD,
    ];
    slice
        .color
        .unwrap_or(Color::hex(FALLBACK[index % FALLBACK.len()]))
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
    let mut chart = Chart::new(opened?);
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

/// Front matter only at the top. A `---` later is a line of the chart.
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

/// The line without a `%%` comment. `None` when nothing is left.
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

struct Header {
    show_data: bool,
    title: Option<Text>,
}

/// `pie`, then `showData` any number of times, then `title` and the rest
/// of the line. `showData` after `title` is part of the title.
fn header_of(line: usize, text: &str) -> Result<Header, Stop> {
    let Some(mut rest) = keyword(text, "pie") else {
        return Err(Stop::Foreign);
    };
    let mut show_data = false;
    let mut title = None;
    while !rest.is_empty() {
        if let Some(next) = keyword(rest, "showData") {
            show_data = true;
            rest = next;
            continue;
        }
        if let Some(next) = keyword(rest, "title") {
            if !next.is_empty() {
                title = Some(text_at(next, line));
            }
            break;
        }
        return Err(Stop::At(Reason {
            line,
            what: "an unknown line",
        }));
    }
    Ok(Header { show_data, title })
}

struct Chart {
    title: Option<Text>,
    show_data: bool,
    in_descr: bool,
    slices: Vec<Slice>,
}

impl Chart {
    fn new(header: Header) -> Self {
        Self {
            title: header.title,
            show_data: header.show_data,
            in_descr: false,
            slices: Vec::new(),
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
        if let Some(rest) = keyword(text, "showData") {
            if rest.is_empty() {
                self.show_data = true;
                return Ok(());
            }
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        }
        if let Some(rest) = keyword(text, "title") {
            if !rest.is_empty() {
                self.title = Some(text_at(rest, line));
            }
            return Ok(());
        }
        self.push(slice_of(line, text)?)
    }

    fn push(&mut self, slice: Slice) -> Result<(), Reason> {
        if self.slices.iter().any(|other| other.label == slice.label) {
            return Err(Reason {
                line: slice.line,
                what: "a repeated slice",
            });
        }
        let total: f32 = self.slices.iter().map(|other| other.value).sum::<f32>() + slice.value;
        if !total.is_finite() {
            return Err(Reason {
                line: slice.line,
                what: "a slice",
            });
        }
        self.slices.push(slice);
        Ok(())
    }

    fn finish(self) -> Diagram {
        Diagram {
            title: self.title,
            show_data: self.show_data,
            slices: self.slices,
            look: Look::default(),
        }
    }
}

fn slice_of(line: usize, text: &str) -> Result<Slice, Reason> {
    let (label, value) = split_slice(text).map_err(|what| Reason { line, what })?;
    let label = unquote(label.trim());
    if label.is_empty() {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    }
    let (value, written) = number(value).map_err(|what| Reason { line, what })?;
    Ok(Slice {
        label,
        value,
        written,
        line,
        color: None,
    })
}

/// The first `:` that is not inside double quotes.
fn split_slice(text: &str) -> Result<(&str, &str), &'static str> {
    let mut quoted = false;
    for (index, ch) in text.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            ':' if !quoted => return Ok((&text[..index], &text[index + 1..])),
            _ => {}
        }
    }
    if quoted {
        Err("a quote")
    } else {
        Err("an unknown line")
    }
}

/// A plain number: digits, one optional leading `-`, and one optional
/// fraction. Anything else, including an exponent, is not a slice value.
fn number(raw: &str) -> Result<(f32, String), &'static str> {
    let written = raw.trim();
    if !plain_number(written) {
        return Err("a slice");
    }
    let value: f32 = written.parse().map_err(|_| "a slice")?;
    if !value.is_finite() {
        return Err("a slice");
    }
    Ok((value, written.to_owned()))
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

    #[test]
    fn the_fixture_is_three_slices_under_its_title() {
        let source = include_str!("../../omaspec-core/tests/fixtures/pie.mmd");
        let diagram = chart(source);
        assert_eq!(
            diagram.title.as_ref().map(|text| text.value.as_str()),
            Some("Disk usage")
        );
        assert!(!diagram.show_data);
        let slices: Vec<_> = diagram
            .slices
            .iter()
            .map(|slice| (slice.label.as_str(), slice.value, slice.written.as_str()))
            .collect();
        assert_eq!(
            slices,
            [
                ("Packages", 42.0, "42"),
                ("Home", 50.0, "50"),
                ("Cache", 8.0, "8")
            ]
        );
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("Disk usage")
                && svg.contains("Packages")
                && svg.contains("Home")
                && svg.contains("Cache"),
            "{svg}"
        );
    }

    #[test]
    fn wedges_start_at_the_top_and_run_clockwise() {
        let svg = draw(&chart("pie\n\"A\" : 1\n\"B\" : 1\n")).unwrap();
        assert!(
            svg.contains(r#"d="M 88,88 L 88,0 A 88,88 0 0,1 88,176 Z""#),
            "{svg}"
        );
        assert!(
            svg.contains(r#"d="M 88,88 L 88,176 A 88,88 0 0,1 88,0 Z""#),
            "{svg}"
        );
    }

    #[test]
    fn a_whole_chart_is_an_ellipse_and_zeros_stay_in_the_legend() {
        let whole = draw(&chart("pie\n\"Only\" : 3\n")).unwrap();
        assert!(whole.contains("<ellipse"), "{whole}");
        assert!(!whole.contains("<path"), "{whole}");

        let empty = draw(&chart("pie title Waiting\n")).unwrap();
        assert!(
            empty.contains("Waiting") && empty.contains("<ellipse"),
            "{empty}"
        );
        assert!(!empty.contains("<path"), "{empty}");

        let zeros = chart("pie\n\"None\" : 0\n\"Also\" : 0\n");
        assert_eq!(zeros.slices.len(), 2);
        let svg = draw(&zeros).unwrap();
        assert!(
            svg.contains("None") && svg.contains("Also") && svg.contains("<ellipse"),
            "{svg}"
        );
        assert!(!svg.contains("<path"), "{svg}");
    }

    #[test]
    fn show_data_prints_the_written_number() {
        let diagram = chart("pie showData title Disk\n\"a:b\" : 1.5\n\"A\" : 1\n\"a\" : 2\n");
        assert!(diagram.show_data);
        assert_eq!(
            diagram.title.as_ref().map(|text| text.value.as_str()),
            Some("Disk")
        );
        assert_eq!(diagram.slices[0].label, "a:b");
        assert_eq!(diagram.slices[0].written, "1.5");
        let svg = draw(&diagram).unwrap();
        assert!(
            svg.contains("a:b [1.5]") && svg.contains("A [1]") && svg.contains("a [2]"),
            "{svg}"
        );
    }

    #[test]
    fn the_last_title_wins_and_notes_are_skipped() {
        let diagram = chart(
            "---\n\
             title: Front\n\
             ---\n\
             pie title Header\n\
             accTitle: hidden\n\
             accDescr {\n\
               several lines\n\
             }\n\
             %% a note\n\
             title Body\n\
             \"A\" : 2 %% trailing\n",
        );
        assert_eq!(
            diagram.title.as_ref().map(|text| text.value.as_str()),
            Some("Body")
        );
        assert_eq!(diagram.slices.len(), 1);
        assert_eq!(diagram.slices[0].label, "A");
        let front = chart("---\ntitle: Front\n---\npie\n\"A\" : 1\n");
        assert_eq!(
            front.title.as_ref().map(|text| text.value.as_str()),
            Some("Front")
        );
        assert!(mermaid_refusal("---\ntitle: Hi\n---\nsequenceDiagram\nA->>B: hi\n").is_none());
        assert!(from_mermaid("sequenceDiagram\nA->>B: hi\n").is_none());
    }

    #[test]
    fn a_negative_slice_stays_in_the_legend() {
        let diagram = chart("pie showData\n\"A\" : 1\n\"B\" : 1\n\"C\" : -1\n");
        assert_eq!(diagram.slices[2].value, -1.0);
        assert_eq!(diagram.slices[2].written, "-1");
        let svg = draw(&diagram).unwrap();
        assert!(svg.contains("C [-1]"), "{svg}");
        assert!(
            svg.contains(r#"d="M 88,88 L 88,0 A 88,88 0 0,1 88,176 Z""#)
                && svg.contains(r#"d="M 88,88 L 88,176 A 88,88 0 0,1 88,0 Z""#),
            "{svg}"
        );
        let only = draw(&chart("pie\n\"Drop\" : -4\n")).unwrap();
        assert!(
            only.contains("Drop") && only.contains("<ellipse") && !only.contains("<path"),
            "{only}"
        );
    }

    #[test]
    fn refuses_a_slice_it_cannot_draw() {
        refuses("pie foo\n", 0, "an unknown line");
        refuses("pie\nclick A\n", 1, "an unknown line");
        refuses("pie\n: 1\n", 1, "an unknown line");
        refuses("pie\n\"\" : 1\n", 1, "an unknown line");
        refuses("pie\n\"A : 1\n", 1, "a quote");
        refuses("pie\n\"A\" : nope\n", 1, "a slice");
        refuses("pie\n\"A\" : 1e2\n", 1, "a slice");
        refuses("pie\n\"A\" : 1\n\"A\" : 2\n", 2, "a repeated slice");
        refuses(
            "---\ndisplayMode: compact\n---\npie\n",
            1,
            "a front matter setting",
        );
    }
}
