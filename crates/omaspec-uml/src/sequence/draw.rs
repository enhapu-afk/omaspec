//! Lays out a parsed sequence diagram and draws it as SVG.
//!
//! First every text is measured. The columns' places then follow from
//! the space each element needs between two lifelines or beside one, in
//! a single pass from left to right. Last the items are drawn from top to
//! bottom; frames, lifelines and activation bars are closed as their end
//! comes, dividers and titles once the width is known.

use super::skin::{Look, MessageAlign, Skin};
use super::{
    Diagram, End, GroupKind, Head, Item, Mark, Message, Note, NotePlace, NoteShape, Shape, Side,
    Text,
};
use crate::Declined;
use crate::annotations::Annotations;
use crate::color::{self, Color, Paint};
use crate::map::{Map, Step};
use crate::svg::{Align, Canvas, Layer, Stroke, n};
use crate::text::{self, Block, Family, Font};

mod names;
mod room;

/// Space around the drawing.
const MARGIN: f32 = 10.0;
/// Between two heads.
const HEAD_GAP: f32 = 10.0;
/// Inside a participant box, around its text.
const HEAD_PAD: f32 = 7.0;
/// A message's text starts this far from the arrow's left end.
const TEXT_INSET: f32 = 7.0;
/// What a message between two lifelines needs besides its text.
const MESSAGE_ROOM: f32 = 26.0;
/// Width of a self message's loop and its height.
const LOOP_WIDTH: f32 = 30.0;
const LOOP_HEIGHT: f32 = 13.0;
/// Width of an activation bar and the shift of a nested one.
const BAR: f32 = 10.0;
const BAR_SHIFT: f32 = 5.0;
/// How far a note or reference over participants reaches past their lifelines.
const OVER: f32 = 15.0;
/// A frame's margin around what it holds.
const FRAME: f32 = 10.0;
/// What the words of a delay keep from a lifeline beside them.
const DELAY_ROOM: f32 = 8.0;
/// Between a page's feet and the next page's title or heads.
const PAGE_GAP: f32 = 30.0;

/// How far the boxes around `b` (Teoz's box inside a box) keep it in on
/// the left and on the right: their padding, where they end at the same
/// column as it does.
fn inset(boxes: &[BoxPlace], b: &BoxPlace, padding: f32) -> (f32, f32) {
    let count = |same: &dyn Fn(&BoxPlace) -> bool| {
        boxes
            .iter()
            .filter(|o| o.depth < b.depth && same(o))
            .count() as f32
            * padding
    };
    (count(&|o| o.first == b.first), count(&|o| o.last == b.last))
}

/// Where one page ends and the next begins (`newpage`), from the top down.
#[derive(Clone, Copy)]
struct PageBreak {
    /// Where the lifelines stop.
    end: f32,
    /// Under the feet of the page that ends.
    below: f32,
    /// Where the next page's boxes start, under its title.
    top: f32,
    /// Where the lifelines start again, under the heads.
    resume: f32,
}

/// What interrupts a lifeline.
#[derive(Clone, Copy)]
enum Break {
    /// A delay, dotted, with where its words stand over the lifelines.
    Delay(Option<[f32; 4]>),
    /// The gap between two pages, where it does not run.
    Page,
}
/// How far a `rect` reaches past what it holds.
const RECT_PAD: f32 = 8.0;
/// Between two notes at one height.
const NOTE_GAP: f32 = 10.0;
/// Arrow heads.
const HEAD_LENGTH: f32 = 10.0;
const HEAD_HALF: f32 = 4.0;

pub fn draw(d: &Diagram) -> Result<String, Declined> {
    draw_mapped(d).map(|(svg, _)| svg)
}

/// The SVG, and where its messages are in it.
pub fn draw_mapped(d: &Diagram) -> Result<(String, Map), Declined> {
    let skin = Skin::from_params(&d.frame.skinparams)?;
    let (mut drawer, mut canvas) = laid(d, &skin)?;
    let map = Map {
        messages: std::mem::take(&mut drawer.steps),
        notes: std::mem::take(&mut drawer.note_steps),
        grounds: std::mem::take(&mut drawer.grounds),
        heads: if drawer.band.is_some() {
            std::mem::take(&mut drawer.heads)
        } else {
            Vec::new()
        },
        bands: drawer.band.into_iter().collect(),
        ..Map::default()
    };
    let map = canvas.place(map, MARGIN);
    Ok((canvas.finish(MARGIN, skin.background), map))
}

/// `d` drawn, and drawn again with the room the drawing found it lacks
/// ([`room`]): the drawer as it last drew, and what it drew on.
fn laid<'a>(d: &'a Diagram, skin: &'a Skin) -> Result<(Drawer<'a>, Canvas), Declined> {
    let canvas = || {
        let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
        // Its elements named, the picture drawn as a whole: a play lifts
        // its veil message by message, no box glides (docs/adr/0037).
        canvas.whole();
        canvas.sketch(skin.sketch);
        canvas.shadows(skin.shadow);
        canvas.faces(&skin.faces);
        canvas
    };
    // Its words are measured once; each drawing starts from them.
    let measured = Drawer::new(d, skin, canvas().family())?;
    let mut extra = room::Extra::default();
    let mut pass = 1;
    let mut short = f32::INFINITY;
    loop {
        let mut canvas = canvas();
        let mut drawer = measured.clone();
        drawer.extra = extra.clone();
        drawer.place();
        drawer.run(&mut canvas);
        let more = drawer.crowded();
        // Done when nothing lacks room, or when the room asked for last
        // took nothing away from what lacks it.
        if more.is_empty() || pass >= room::PASSES || more.short >= short - 0.01 {
            return Ok((drawer, canvas));
        }
        short = more.short;
        extra.extend(more);
        pass += 1;
    }
}

/// The font of a message's number: the arrows' words'; in a circle
/// (Mermaid's) a little smaller, in the color that reads best on the
/// circle, which the message's arrow color fills.
fn number_font(skin: &Skin, circled: bool, color: Option<Color>) -> Font {
    if !circled {
        return skin.arrow_font;
    }
    let fill = color.unwrap_or(skin.arrow);
    let ink = [Color::WHITE, Color::BLACK]
        .into_iter()
        .max_by(|a, b| a.contrast(fill).total_cmp(&b.contrast(fill)))
        .unwrap_or(Color::WHITE);
    Font {
        size: (skin.arrow_font.size - 2.0).max(8.0),
        color: ink,
        ..skin.arrow_font
    }
}

/// The radius of the circle a message's `number` stands in (Mermaid's).
fn circle_radius(number: &Block) -> f32 {
    (number.width / 2.0 + 3.0).max(number.height / 2.0).max(7.0)
}

fn measure(text: &Text, font: Font, family: Family) -> Result<Block, Declined> {
    text::block(&text.value, font, family).map_err(|e| Declined::at(text.line, e))
}

fn measure_opt(text: Option<&Text>, font: Font, family: Family) -> Result<Block, Declined> {
    text.map_or(Ok(Block::empty()), |t| measure(t, font, family))
}

/// `[label]`, as groups show their condition.
fn bracketed(text: Option<&Text>, font: Font, family: Family) -> Result<Block, Declined> {
    match text {
        Some(t) => measure(
            &Text {
                value: format!("[{}]", t.value),
                line: t.line,
                col: None,
            },
            font,
            family,
        ),
        None => Ok(Block::empty()),
    }
}

fn paint_at(value: Option<&String>, line: usize) -> Result<Option<Paint>, Declined> {
    value
        .map(|v| color::parse(v).map_err(|e| Declined::at(line, e)))
        .transpose()
}

fn solid_at(value: Option<&String>, line: usize) -> Result<Option<Color>, Declined> {
    match paint_at(value, line)? {
        Some(None) => Err(Declined::at(line, "an arrow cannot be transparent")),
        Some(Some(c)) => Ok(Some(c)),
        None => Ok(None),
    }
}

/// A fill mixed onto `background` when it is translucent. Opaque colors
/// stay as written; a fully transparent one is no fill. The picture has
/// no fill-opacity, so a Mermaid `rgba` highlight is baked here.
fn blended(value: &str, line: usize, background: Paint) -> Result<Paint, Declined> {
    let (paint, alpha) = color::parse_ink(value).map_err(|e| Declined::at(line, e))?;
    Ok(match paint {
        Some(color) if alpha == 255 => Some(color),
        Some(color) if alpha > 0 => {
            let onto = background.unwrap_or(Color::WHITE);
            Some(onto.mix(color, f32::from(alpha) / 255.0))
        }
        _ => None,
    })
}

/// A participant drawn as a column.
#[derive(Clone)]
struct Column {
    shape: Shape,
    label: Block,
    stereo: Block,
    spot: Option<SpotMark>,
    /// The line the spot and the stereotype share.
    stereo_w: f32,
    stereo_h: f32,
    look: Look,
    width: f32,
    height: f32,
    /// The item of the message that creates it, if it is created later.
    created_by: Option<usize>,
    /// Followed on a click on its head.
    link: Option<crate::text::Link>,
}

/// A stereotype's spot as drawn: its letter in a circle.
#[derive(Clone)]
struct SpotMark {
    letter: Block,
    fill: Paint,
    size: f32,
}

/// The spot's green when its color is not written, as PlantUML's.
const SPOT_GREEN: Color = Color::hex(0xADD1B2);
/// Between a spot and its stereotype.
const SPOT_GAP: f32 = 4.0;

impl Column {
    fn half(&self) -> f32 {
        self.width / 2.0
    }
}

/// What was measured for an item.
#[derive(Clone)]
enum Measured {
    Message {
        text: Block,
        number: Block,
        color: Option<Color>,
    },
    Note {
        text: Block,
        width: f32,
        height: f32,
        fill: Paint,
    },
    Group {
        header: Block,
        label: Block,
        /// The tab's color, and the one behind the first part.
        tab: Paint,
        fill: Paint,
    },
    Else {
        label: Block,
        /// The color behind this part.
        fill: Paint,
    },
    Label(Block),
    Ref {
        text: Block,
        header: Block,
        fill: Paint,
    },
    Activate {
        fill: Paint,
    },
    /// A `rect` highlight, already mixed onto the page background.
    Rect {
        fill: Paint,
    },
    Nothing,
}

/// Size of the drawn icon of the shapes that have one.
fn icon(shape: Shape) -> Option<(f32, f32)> {
    match shape {
        Shape::Actor => Some((26.0, 50.0)),
        Shape::Boundary => Some((34.0, 24.0)),
        Shape::Control => Some((24.0, 26.0)),
        Shape::Entity => Some((24.0, 28.0)),
        Shape::Database => Some((30.0, 38.0)),
        _ => None,
    }
}

/// The room each column needs to its neighbours and the edges.
struct Space {
    into: Vec<Vec<(usize, f32)>>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Space {
    fn new(n: usize) -> Space {
        Space {
            into: vec![Vec::new(); n],
            left: vec![0.0; n],
            right: vec![0.0; n],
        }
    }

    /// Lifelines `a` and `b` at least `d` apart.
    fn need(&mut self, a: usize, b: usize, d: f32) {
        if a != b {
            self.into[a.max(b)].push((a.min(b), d));
        }
    }

    /// `d` free on the left of column `i`.
    fn before(&mut self, i: usize, d: f32) {
        if i > 0 {
            self.need(i - 1, i, d);
        }
        self.left[i] = self.left[i].max(d);
    }

    /// `d` free on the right of column `i`.
    fn after(&mut self, i: usize, d: f32) {
        if i + 1 < self.into.len() {
            self.need(i, i + 1, d);
        }
        self.right[i] = self.right[i].max(d);
    }

    /// The lifelines' places and the right edge.
    fn solve(&self) -> (Vec<f32>, f32) {
        let mut x: Vec<f32> = Vec::with_capacity(self.into.len());
        for (i, into) in self.into.iter().enumerate() {
            let at = into
                .iter()
                .map(|&(j, d)| x[j] + d)
                .fold(self.left[i], f32::max);
            x.push(at);
        }
        let right = x
            .iter()
            .zip(&self.right)
            .map(|(x, r)| x + r)
            .fold(0.0, f32::max);
        (x, right)
    }
}

/// A box around participants, by columns.
#[derive(Clone)]
struct BoxPlace {
    /// Which of the diagram's boxes it is ([`names::Names::group_box`]).
    of: usize,
    first: usize,
    last: usize,
    title: Block,
    look: Look,
    /// How many boxes stand around it.
    depth: usize,
}

/// An activation bar.
#[derive(Clone)]
struct Bar {
    column: usize,
    depth: usize,
    start: f32,
    end: f32,
    fill: Paint,
    /// The line that opened it.
    line: usize,
}

#[derive(Clone)]
struct OpenGroup {
    top: f32,
    header: Block,
    label: Block,
    /// The tab's color, and the one behind the first part.
    tab: Paint,
    fill: Paint,
    /// The line that opens it.
    line: usize,
    /// Its element's name ([`names::Names::item`]).
    part: String,
    /// Each `else`: where, its words and its line.
    elses: Vec<(f32, Block, usize)>,
    /// The color behind each `else`'s part.
    else_fills: Vec<Paint>,
    /// What it holds, left and right.
    lo: f32,
    hi: f32,
    /// The lifelines its items stand by, the first and the last.
    cols: Option<(usize, usize)>,
    /// Whether it holds a message from or to the picture's left edge, and
    /// its right edge: there it reaches the edge, no further.
    edge: [bool; 2],
}

#[derive(Clone)]
struct Drawer<'a> {
    d: &'a Diagram,
    skin: &'a Skin,
    /// Each element by its name from the source (docs/adr/0037), and
    /// each column's participant's.
    names: names::Names,
    column_names: Vec<String>,
    cols: Vec<Column>,
    col_of: Vec<Option<usize>>,
    measured: Vec<Measured>,
    boxes: Vec<BoxPlace>,
    /// Extra room beside a column's head, for boxes.
    pad_left: Vec<f32>,
    pad_right: Vec<f32>,
    annotations: Annotations,
    /// Labels that the drawing must be wide enough for.
    min_width: f32,

    x: Vec<f32>,
    right_edge: f32,

    y: f32,
    open_bars: Vec<Vec<(f32, Paint, usize)>>,
    bars: Vec<Bar>,
    life_start: Vec<f32>,
    life_end: Vec<Option<f32>>,
    head_drawn: Vec<bool>,
    /// Each delay, from where to where down, and where its words stand
    /// over the lifelines when no room between two holds them: the dotted
    /// lifelines stop at them.
    delays: Vec<(f32, f32, Option<[f32; 4]>)>,
    /// The breaks between pages (`newpage`).
    pages: Vec<PageBreak>,
    /// Teoz's anchors (`{start} A -> B`): the height of each one's arrow
    /// and the middle of it, where a duration from it stands.
    anchors: std::collections::HashMap<String, (f32, f32)>,
    groups: Vec<OpenGroup>,
    dividers: Vec<(f32, Block, usize, String)>,
    /// The arrow height of the last message and its columns, while only
    /// activations follow it.
    last_arrow: Option<(f32, [Option<usize>; 2])>,
    /// Where the last message or note began, for the ones a `&` or a `/`
    /// puts beside it.
    level: Option<Level>,
    /// By item: whether it stands at the height of the one before
    /// ([`beside`]).
    beside: Vec<bool>,
    /// Each message's arrow and band, for the map.
    steps: Vec<Step>,
    /// Each note of its own, a step of a play without an arrow, for the
    /// map's [`Map::notes`].
    note_steps: Vec<Step>,
    /// The boxes with a fill, for the map.
    grounds: Vec<crate::map::Ground>,
    /// The participants' heads at the top, and the boxes' titles over
    /// them, over the lifelines: what a pane keeps in sight
    /// ([`crate::map::Map::bands`]).
    band: Option<crate::map::Band>,
    /// The participants at the top and the names of the boxes, each on
    /// its own, for the map.
    heads: Vec<crate::map::Heading>,
    /// `rect` highlights open at the top, closed into [`Self::highlights`].
    open_rects: Vec<OpenRect>,
    /// Closed highlights, inner ones first. Drawn back to front.
    highlights: Vec<Highlight>,
    /// Where the lifelines that are not destroyed end.
    bottom: f32,
    /// The frames and the notes beside lifelines as drawn ([`room`]).
    drawn: room::Drawn,
    /// Room the columns keep beyond what the items ask for, found by
    /// drawing ([`room`]).
    extra: room::Extra,
}

/// The height messages and notes share: one line, and each `&` or `/`
/// line after it.
#[derive(Clone, Copy)]
struct Level {
    /// Where the first of them began.
    top: f32,
    /// From there down to the arrows, under the tallest of their texts.
    drop: f32,
}

/// By item, whether it stands at the height of the one before: a message
/// or a note with `&` or `/`. A note that would lie on a note of that
/// height (both over one participant, or one of them across them all)
/// does not: it stands below, where it can be read.
fn beside(d: &Diagram, col_of: &[Option<usize>], n: usize) -> Vec<bool> {
    let col = |p: usize| col_of.get(p).copied().flatten().unwrap_or(0);
    // Three places to each column: left of its lifeline, on it, right of it.
    let mut row: Vec<(usize, usize)> = Vec::new();
    d.items
        .iter()
        .map(|item| match item {
            Item::Note(note) => {
                let span = match note.place {
                    NotePlace::LeftOf(p) => (3 * col(p), 3 * col(p)),
                    NotePlace::RightOf(p) => (3 * col(p) + 2, 3 * col(p) + 2),
                    NotePlace::Over(a, b) => {
                        let (a, b) = (col(a), col(b));
                        (3 * a.min(b) + 1, 3 * a.max(b) + 1)
                    }
                    NotePlace::Across => (0, 3 * n.saturating_sub(1) + 2),
                    // Beside its message, whose height it has anyway.
                    NotePlace::Message(_) => return note.together,
                };
                let fits = note.together && row.iter().all(|old| old.1 < span.0 || span.1 < old.0);
                if !fits {
                    row.clear();
                }
                row.push(span);
                fits
            }
            Item::Message(m) => {
                if !m.together {
                    row.clear();
                }
                m.together
            }
            Item::Activate { .. } | Item::Deactivate { .. } | Item::Destroy { .. } => false,
            _ => {
                row.clear();
                false
            }
        })
        .collect()
}

/// The columns a note covers, and how far it reaches past the first to
/// the left and past the last to the right.
struct Reach {
    lo: usize,
    hi: usize,
    left: f32,
    right: f32,
}

/// A `rect` waiting for its `end`.
#[derive(Clone)]
struct OpenRect {
    top: f32,
    fill: Paint,
    /// What it holds, left and right.
    lo: f32,
    hi: f32,
}

/// One closed `rect`: as wide as what it holds, with [`RECT_PAD`] around,
/// or across the participant band when it holds nothing.
#[derive(Clone)]
struct Highlight {
    top: f32,
    bottom: f32,
    left: f32,
    right: f32,
    fill: Paint,
}

impl<'a> Drawer<'a> {
    fn new(d: &'a Diagram, skin: &'a Skin, family: Family) -> Result<Self, Declined> {
        // Which participants the items use, and the messages that create them.
        let mut used = vec![false; d.participants.len()];
        let mut created_by: Vec<Option<usize>> = vec![None; d.participants.len()];
        for (k, item) in d.items.iter().enumerate() {
            for p in participants_of(item) {
                if let Some(u) = used.get_mut(p) {
                    *u = true;
                }
            }
            if let Item::Message(m) = item
                && m.creates
                && let End::Participant(p) = m.to
                && let Some(slot) = created_by.get_mut(p)
            {
                *slot = Some(k);
            }
        }

        let mut cols = Vec::new();
        let mut col_of = vec![None; d.participants.len()];
        for (i, p) in d.participants.iter().enumerate() {
            if d.hide_unlinked && !used[i] {
                continue;
            }
            let mut look = *skin.look(p.shape);
            if let Some(fill) = paint_at(p.color.as_ref(), p.line)? {
                look.fill = fill;
            }
            // `skinparam sequenceParticipant underline`: each line of it.
            let value = if skin.participant_underline {
                // Each line, broken by a line of its own or by `\n`.
                p.label
                    .replace("\\n", "\n")
                    .split('\n')
                    .map(|line| format!("<u>{line}</u>"))
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                p.label.clone()
            };
            let label = measure(
                &Text {
                    value,
                    line: p.line,
                    col: None,
                },
                look.font,
                family,
            )?;
            let stereo = match &p.stereotype {
                Some(s) => measure(
                    &Text {
                        value: if skin.guillemet {
                            format!("«{s}»")
                        } else if p.stereotype_spaced {
                            format!("<< {s} >>")
                        } else {
                            format!("<<{s}>>")
                        },
                        line: p.line,
                        col: None,
                    },
                    skin.stereotype_font,
                    family,
                )?,
                None => Block::empty(),
            };
            let spot = match &p.spot {
                Some(spot) => {
                    let font = Font {
                        italic: false,
                        ..skin.stereotype_font
                    }
                    .bold();
                    let letter = measure(
                        &Text {
                            value: spot.letter.to_string(),
                            line: p.line,
                            col: None,
                        },
                        font,
                        family,
                    )?;
                    let fill = paint_at(spot.color.as_ref(), p.line)?.unwrap_or(Some(SPOT_GREEN));
                    let size = letter.width.max(letter.height) + 4.0;
                    Some(SpotMark { letter, fill, size })
                }
                None => None,
            };
            let (stereo_w, stereo_h) = match &spot {
                Some(spot) if stereo.is_empty() => (spot.size, spot.size),
                Some(spot) => (
                    spot.size + SPOT_GAP + stereo.width,
                    spot.size.max(stereo.height),
                ),
                None => (stereo.width, stereo.height),
            };
            let text_w = label.width.max(stereo_w);
            let text_h = label.height + stereo_h;
            let (width, height) = match (p.shape, icon(p.shape)) {
                (_, Some((w, h))) => (w.max(text_w), h + 3.0 + text_h),
                (Shape::Collections, _) => {
                    (text_w + 2.0 * HEAD_PAD + 4.0, text_h + 2.0 * HEAD_PAD + 4.0)
                }
                (Shape::Queue, _) => (text_w + 2.0 * HEAD_PAD + 10.0, text_h + 2.0 * HEAD_PAD),
                _ => (text_w + 2.0 * HEAD_PAD, text_h + 2.0 * HEAD_PAD),
            };
            col_of[i] = Some(cols.len());
            cols.push(Column {
                shape: p.shape,
                label,
                stereo,
                spot,
                stereo_w,
                stereo_h,
                look,
                width,
                height,
                created_by: created_by[i],
                link: p.link.clone(),
            });
        }
        if cols.is_empty() {
            // A diagram of nothing is empty, whatever type it would become;
            // PlantUML shows its welcome instead. A title, a header or a
            // legend alone PlantUML draws, so the Java PlantUML may.
            let f = &d.frame;
            let words = f.title.is_some()
                || f.header.is_some()
                || f.footer.is_some()
                || f.caption.is_some()
                || f.legend.is_some()
                || f.mainframe.is_some();
            let (reason, refused) = match (d.items.is_empty(), words) {
                (true, false) => ("the diagram is empty", true),
                (true, true) => (
                    "the diagram has nothing but its title, header or legend yet",
                    false,
                ),
                (false, _) => ("a sequence diagram needs a participant", false),
            };
            return Err(Declined {
                line: None,
                reason: reason.into(),
                refused,
            });
        }

        let mut measured = Vec::with_capacity(d.items.len());
        for item in &d.items {
            measured.push(match item {
                Item::Message(m) => {
                    let color = solid_at(m.arrow.color.as_ref(), m.line)?;
                    Measured::Message {
                        text: match (skin.max_message_size, &m.text) {
                            // `skinparam maxMessageSize`: wrapped at its spaces.
                            (Some(width), Some(t)) => {
                                text::wrapped(&t.value, skin.arrow_font, family, width)
                                    .map_err(|e| Declined::at(t.line, e))?
                            }
                            _ => measure_opt(m.text.as_ref(), skin.arrow_font, family)?,
                        },
                        number: match &m.number {
                            Some(number) => measure(
                                &Text {
                                    value: number.clone(),
                                    line: m.line,
                                    col: None,
                                },
                                number_font(skin, d.numbers_circled, color),
                                family,
                            )?,
                            None => Block::empty(),
                        },
                        color,
                    }
                }
                Item::Note(note) => {
                    let text = measure(&note.text, skin.note.font, family)?;
                    let (width, height) = note_size(note.shape, &text);
                    let fill = paint_at(note.color.as_ref(), note.line)?.unwrap_or(skin.note.fill);
                    Measured::Note {
                        text,
                        width,
                        height,
                        fill,
                    }
                }
                Item::GroupStart {
                    kind,
                    label,
                    secondary,
                    tab,
                    fill,
                    line,
                } => {
                    let (header, label) = if *kind == GroupKind::Group {
                        let header = match label {
                            Some(l) => measure(l, skin.group_header_font, family)?,
                            None => {
                                measure(&keyword(*kind, *line), skin.group_header_font, family)?
                            }
                        };
                        (
                            header,
                            bracketed(secondary.as_ref(), skin.group_label_font, family)?,
                        )
                    } else {
                        (
                            measure(&keyword(*kind, *line), skin.group_header_font, family)?,
                            bracketed(label.as_ref(), skin.group_label_font, family)?,
                        )
                    };
                    Measured::Group {
                        header,
                        label,
                        tab: paint_at(tab.as_ref(), *line)?.unwrap_or(skin.group_header_fill),
                        fill: paint_at(fill.as_ref(), *line)?.flatten(),
                    }
                }
                Item::GroupElse { label, fill, line } => Measured::Else {
                    label: bracketed(label.as_ref(), skin.group_label_font, family)?,
                    fill: paint_at(fill.as_ref(), *line)?.flatten(),
                },
                Item::Divider { label } => {
                    Measured::Label(measure(label, skin.divider.font, family)?)
                }
                Item::Delay { label, .. } => {
                    Measured::Label(measure_opt(label.as_ref(), skin.delay_font, family)?)
                }
                Item::NewPage { title, .. } => {
                    Measured::Label(measure_opt(title.as_ref(), skin.title_font, family)?)
                }
                Item::Duration { label, .. } => {
                    Measured::Label(measure_opt(label.as_ref(), skin.arrow_font, family)?)
                }
                Item::Ref { text, color, .. } => Measured::Ref {
                    text: measure(text, skin.reference.font, family)?,
                    header: measure(
                        &Text {
                            value: "ref".into(),
                            line: text.line,
                            col: None,
                        },
                        skin.group_header_font,
                        family,
                    )?,
                    fill: paint_at(color.as_ref(), text.line)?.unwrap_or(skin.reference.fill),
                },
                Item::Activate { color, line, .. } => Measured::Activate {
                    fill: paint_at(color.as_ref(), *line)?.unwrap_or(skin.activation_fill),
                },
                Item::RectStart { color, line } => Measured::Rect {
                    fill: blended(color, *line, skin.background)?,
                },
                _ => Measured::Nothing,
            });
        }

        let mut boxes = Vec::new();
        let n = cols.len();
        let mut pad_left = vec![0.0; n];
        let mut pad_right = vec![0.0; n];
        for (of, b) in d.boxes.iter().enumerate() {
            let mut members: Vec<usize> = b
                .participants
                .iter()
                .filter_map(|&p| col_of.get(p).copied().flatten())
                .collect();
            members.sort_unstable();
            members.dedup();
            let (Some(&first), Some(&last)) = (members.first(), members.last()) else {
                continue;
            };
            if last - first + 1 != members.len() {
                return Err(Declined::at(
                    b.line,
                    "the participants of a box must stand next to each other",
                ));
            }
            let mut look = skin.box_look;
            if let Some(color) = &b.color {
                look.fill = blended(color, b.line, skin.background)?;
            }
            let title = measure_opt(b.title.as_ref(), look.font, family)?;
            pad_left[first] += skin.box_padding;
            pad_right[last] += skin.box_padding;
            let place = BoxPlace {
                of,
                first,
                last,
                title,
                look,
                depth: b.depth,
            };
            // A box of one keeps room for its title, inside those around it.
            if first == last {
                let more =
                    ((place.title.width + 10.0 - cols[first].width) / 2.0).max(skin.box_padding);
                let (left, right) = inset(&boxes, &place, skin.box_padding);
                pad_left[first] = pad_left[first].max(more + left);
                pad_right[first] = pad_right[first].max(more + right);
            }
            boxes.push(place);
        }

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
        // Dividers reach as far as the titles, unless a main frame holds
        // them.
        let min_width = if annotations.framed() {
            0.0
        } else {
            annotations.width()
        };

        let beside = beside(d, &col_of, n);
        let names = names::Names::of(d);
        let column_names = (0..cols.len())
            .map(|c| {
                (0..d.participants.len())
                    .find(|&p| col_of[p] == Some(c))
                    .map_or_else(String::new, |p| names.participant(p).to_owned())
            })
            .collect();
        Ok(Drawer {
            d,
            skin,
            names,
            column_names,
            col_of,
            measured,
            boxes,
            pad_left,
            pad_right,
            annotations,
            min_width,
            x: Vec::new(),
            right_edge: 0.0,
            y: 0.0,
            open_bars: vec![Vec::new(); n],
            bars: Vec::new(),
            life_start: vec![0.0; n],
            life_end: vec![None; n],
            head_drawn: vec![false; n],
            delays: Vec::new(),
            pages: Vec::new(),
            anchors: std::collections::HashMap::new(),
            groups: Vec::new(),
            dividers: Vec::new(),
            last_arrow: None,
            level: None,
            beside,
            steps: Vec::new(),
            note_steps: Vec::new(),
            grounds: Vec::new(),
            band: None,
            heads: Vec::new(),
            open_rects: Vec::new(),
            highlights: Vec::new(),
            bottom: 0.0,
            drawn: room::Drawn::default(),
            extra: room::Extra::default(),
            cols,
        })
    }

    fn col(&self, participant: usize) -> usize {
        self.col_of.get(participant).copied().flatten().unwrap_or(0)
    }

    fn end_col(&self, end: End) -> Option<usize> {
        match end {
            End::Participant(p) => Some(self.col(p)),
            _ => None,
        }
    }

    /// The room a message's text takes, with its number.
    fn message_text_width(&self, k: usize) -> f32 {
        match &self.measured[k] {
            // A number in a circle stands at the arrow, not with the words.
            Measured::Message { text, .. } if self.d.numbers_circled => text.width,
            Measured::Message { text, number, .. } => {
                let gap = if number.is_empty() || text.is_empty() {
                    0.0
                } else {
                    4.0
                };
                text.width + number.width + gap
            }
            _ => 0.0,
        }
    }

    /// How far from the start of its arrow a message's words keep, clear
    /// of the circle its number stands in (Mermaid's); none without one.
    fn circled(&self, k: usize) -> Option<f32> {
        match &self.measured[k] {
            Measured::Message { number, .. } if self.d.numbers_circled && !number.is_empty() => {
                Some(circle_radius(number) + 2.0)
            }
            _ => None,
        }
    }

    /// From where a message begins down to its arrow: its text is above,
    /// but for an answer whose words go under it.
    fn arrow_drop(&self, k: usize) -> Option<f32> {
        match &self.measured[k] {
            Measured::Message { .. } if self.words_below(k) => Some(3.0),
            Measured::Message { text, number, .. } => {
                let height = text.height.max(number.height);
                Some(height + if height > 0.0 { 2.0 } else { 3.0 })
            }
            _ => None,
        }
    }

    /// `skinparam responseMessageBelowArrow`: whether the message at `k`
    /// points to the left, an answer, and has its words under its arrow.
    fn words_below(&self, k: usize) -> bool {
        let Item::Message(m) = &self.d.items[k] else {
            return false;
        };
        // As PlantUML reads it: written with its head on the left
        // (`B <- A`), between two participants, whichever way it points.
        self.skin.response_below
            && m.written_back
            && matches!((m.from, m.to), (End::Participant(a), End::Participant(b)) if a != b)
    }

    /// The deepest [`Self::arrow_drop`] of the item at `k` and of those a
    /// `&` puts beside it.
    fn shared_drop(&self, k: usize) -> f32 {
        let beside = self.d.items[k + 1..]
            .iter()
            .enumerate()
            .take_while(|(i, item)| match item {
                Item::Message(_) => self.beside[k + 1 + *i],
                Item::Note(note) => {
                    self.beside[k + 1 + *i] || matches!(note.place, NotePlace::Message(_))
                }
                Item::Activate { .. } | Item::Deactivate { .. } | Item::Destroy { .. } => true,
                _ => false,
            })
            .map(|(i, _)| k + 1 + i);
        std::iter::once(k)
            .chain(beside)
            .filter_map(|i| self.arrow_drop(i))
            .fold(0.0, f32::max)
    }

    /// Before the message or note at `k` is drawn: with `together` it goes
    /// back up to the height of the one before, and what is returned is
    /// how far down the others reach; otherwise a new height begins here.
    /// A message starts lower by what its text is shorter than the
    /// tallest, so that the arrows lie on one line.
    fn share(&mut self, k: usize, together: bool) -> Option<f32> {
        let below = match self.level {
            Some(level) if together => {
                let below = self.y;
                self.y = level.top;
                Some(below)
            }
            _ => {
                self.level = Some(Level {
                    top: self.y,
                    drop: self.shared_drop(k),
                });
                None
            }
        };
        if let (Some(own), Some(level)) = (self.arrow_drop(k), self.level) {
            self.y += (level.drop - own).max(0.0);
        }
        below
    }

    /// Places the columns.
    fn place(&mut self) {
        let d = self.d;
        let n = self.cols.len();
        let mut space = Space::new(n);
        let pad = self.skin.participant_padding;
        for i in 0..n {
            let half = self.cols[i].half();
            space.left[i] = half + self.pad_left[i] + pad;
            space.right[i] = half + self.pad_right[i] + pad;
            if i + 1 < n {
                let next = self.cols[i + 1].half();
                let boxed = if self.pad_right[i] > 0.0 || self.pad_left[i + 1] > 0.0 {
                    5.0
                } else {
                    0.0
                };
                space.need(
                    i,
                    i + 1,
                    half + self.pad_right[i]
                        + next
                        + self.pad_left[i + 1]
                        + HEAD_GAP
                        + 2.0 * pad
                        + boxed,
                );
            }
        }
        for b in &self.boxes {
            if b.first < b.last {
                // Inside the boxes around it, it has less of the padding.
                let (left, right) = inset(&self.boxes, b, self.skin.box_padding);
                let inner = self.cols[b.first].half() + self.pad_left[b.first] - left
                    + self.cols[b.last].half()
                    + self.pad_right[b.last]
                    - right;
                space.need(b.first, b.last, b.title.width + 10.0 - inner);
            }
        }

        // Groups: the columns they hold, to fit their header.
        // Open groups: the columns they hold so far, the width they need.
        // Each open frame: the columns it spans, the width its tab and
        // labels need, the frames closed inside it, each with its columns
        // and how many frames stand inside it at either side, and the
        // participants created inside it, whose heads it holds.
        type Closed = ((usize, usize), (usize, usize));
        type Opened = (Option<(usize, usize)>, f32, Vec<Closed>, Vec<usize>);
        let mut open: Vec<Opened> = Vec::new();
        for (k, item) in d.items.iter().enumerate() {
            let cols: Vec<usize> = participants_of(item)
                .into_iter()
                .map(|p| self.col(p))
                .collect();
            for (_, _, _, heads) in open.iter_mut() {
                heads.extend(cols.iter().filter(|&&c| self.cols[c].created_by == Some(k)));
            }
            for (span, _, _, _) in open.iter_mut() {
                for &c in &cols {
                    *span = Some(match *span {
                        Some((lo, hi)) => (lo.min(c), hi.max(c)),
                        None => (c, c),
                    });
                }
            }
            match (item, &self.measured[k]) {
                (Item::GroupStart { .. }, Measured::Group { header, label, .. }) => {
                    let width = header.width
                        + 18.0
                        + if label.is_empty() {
                            0.0
                        } else {
                            label.width + 5.0
                        };
                    open.push((None, width + 10.0, Vec::new(), Vec::new()));
                }
                (Item::GroupElse { .. }, Measured::Else { label, .. }) => {
                    if let Some(last) = open.last_mut() {
                        last.1 = last.1.max(label.width + 15.0);
                    }
                }
                (Item::GroupEnd { .. }, _) => {
                    if let Some((span, width, inner, heads)) = open.pop() {
                        let (lo, hi) = span.unwrap_or((0, n - 1));
                        // The frames inside that share a side with this one
                        // stand in its margin there.
                        let left = inner
                            .iter()
                            .filter(|((at, _), _)| *at == lo)
                            .map(|((_, depth), _)| depth + 1)
                            .max()
                            .unwrap_or(0);
                        let right = inner
                            .iter()
                            .filter(|(_, (at, _))| *at == hi)
                            .map(|(_, (_, depth))| depth + 1)
                            .max()
                            .unwrap_or(0);
                        // Its sides keep their margin from what stands at
                        // the lifeline they stand by, a bar or a head made
                        // inside, and from a bar on the lifeline beside,
                        // and stay in the picture.
                        let room = |c: usize, depth: usize| {
                            let half = if heads.contains(&c) {
                                self.cols[c].half()
                            } else {
                                BAR / 2.0
                            };
                            FRAME * (depth + 1) as f32 + half
                        };
                        let beside = BAR / 2.0 + FRAME;
                        let (at_lo, at_hi) = (room(lo, left), room(hi, right));
                        if lo > 0 {
                            space.need(lo - 1, lo, at_lo + beside);
                        }
                        space.left[lo] = space.left[lo].max(at_lo);
                        if hi + 1 < n {
                            space.need(hi, hi + 1, at_hi + beside);
                        }
                        space.right[hi] = space.right[hi].max(at_hi);
                        // The words of its tab and of its `else`s reach
                        // `width` from its left side, their margin to its
                        // right side with them. Over several lifelines
                        // they end that margin before the last one, clear
                        // of a bar on it, also where no bar pushes the
                        // left side out; over one, the frame holds them
                        // right of it, clear of the lifeline beside.
                        if lo < hi {
                            space.need(lo, hi, width - at_lo + BAR / 2.0);
                        } else {
                            let past = width - at_lo;
                            if lo + 1 < n {
                                space.need(lo, lo + 1, past + beside);
                            }
                            space.right[lo] = space.right[lo].max(past);
                        }
                        if let Some(outer) = open.last_mut() {
                            outer.2.push(((lo, left), (hi, right)));
                        }
                    }
                }
                _ => {}
            }
        }

        let mut min_width = self.min_width;
        for (k, item) in d.items.iter().enumerate() {
            match item {
                Item::Message(m) => {
                    // Its words keep clear of the circle its number stands
                    // in, further than they keep from the arrow's start.
                    let clear = self
                        .circled(k)
                        .map_or(0.0, |keep| (keep - TEXT_INSET).max(0.0));
                    let w = self.message_text_width(k) + clear;
                    let room = w + MESSAGE_ROOM;
                    let (from, to) = (self.end_col(m.from), self.end_col(m.to));
                    let (lo, hi, self_loop) = match (from, to) {
                        (Some(a), Some(b)) if a == b => {
                            let reach = (w + 2.0 * TEXT_INSET).max(LOOP_WIDTH + 10.0) + BAR;
                            space.after(a, reach + 5.0);
                            (a, a, Some(reach))
                        }
                        (Some(a), Some(b)) => {
                            // A head it creates takes the half of it its
                            // arrow ends at the side of.
                            let head = if self.cols[b].created_by == Some(k) {
                                self.cols[b].half()
                            } else {
                                0.0
                            };
                            space.need(a, b, room + BAR + head);
                            (a.min(b), a.max(b), None)
                        }
                        (Some(c), None) | (None, Some(c)) => {
                            let other = if from.is_some() { m.to } else { m.from };
                            match other {
                                End::Edge(Side::Left) => space.left[c] = space.left[c].max(room),
                                End::Edge(Side::Right) => {
                                    space.right[c] = space.right[c].max(room);
                                }
                                End::Short(Side::Left) => space.before(c, room.max(30.0) + 5.0),
                                End::Short(Side::Right) => space.after(c, room.max(30.0) + 5.0),
                                End::Participant(_) => {}
                            }
                            (c, c, None)
                        }
                        (None, None) => continue,
                    };
                    if let Some(Item::Note(note)) = d.items.get(k + 1)
                        && let NotePlace::Message(side) = note.place
                        && let Measured::Note { width, .. } = self.measured[k + 1]
                    {
                        match side {
                            Side::Left => space.before(lo, width + 15.0),
                            Side::Right => space.after(hi, self_loop.unwrap_or(0.0) + width + 15.0),
                        }
                    }
                }
                Item::Note(note) => {
                    let Measured::Note { width, .. } = self.measured[k] else {
                        continue;
                    };
                    match note.place {
                        NotePlace::LeftOf(p) => space.before(self.col(p), width + 10.0),
                        NotePlace::RightOf(p) => space.after(self.col(p), width + 10.0),
                        NotePlace::Over(a, b) => {
                            let (a, b) = (self.col(a), self.col(b));
                            over(&mut space, a.min(b), a.max(b), width);
                        }
                        NotePlace::Across => over(&mut space, 0, n - 1, width),
                        NotePlace::Message(_) => {}
                    }
                }
                Item::Ref { over: list, .. } => {
                    let Measured::Ref { text, header, .. } = &self.measured[k] else {
                        continue;
                    };
                    let width = (text.width + 20.0).max(header.width + 30.0);
                    let cols: Vec<usize> = list.iter().map(|&p| self.col(p)).collect();
                    let lo = cols.iter().copied().min().unwrap_or(0);
                    let hi = cols.iter().copied().max().unwrap_or(0);
                    over(&mut space, lo, hi, width);
                }
                Item::Delay { .. } | Item::Divider { .. } => {
                    if let Measured::Label(label) = &self.measured[k] {
                        let width = label.width + 40.0;
                        if n > 1 {
                            space.need(0, n - 1, width - 2.0 * OVER);
                        }
                        min_width = min_width.max(width);
                    }
                }
                _ => {}
            }
        }

        // Notes at one height stand beside one another.
        let mut row: Vec<Reach> = Vec::new();
        for (k, item) in d.items.iter().enumerate() {
            match item {
                Item::Note(note) => {
                    if !self.beside[k] {
                        row.clear();
                    }
                    let Measured::Note { width, .. } = self.measured[k] else {
                        continue;
                    };
                    let (lo, hi, left, right) = match note.place {
                        NotePlace::LeftOf(p) => (self.col(p), self.col(p), width + BAR, 0.0),
                        NotePlace::RightOf(p) => (self.col(p), self.col(p), 0.0, width + 2.0 * BAR),
                        NotePlace::Over(a, b) if a == b => {
                            (self.col(a), self.col(a), width / 2.0, width / 2.0)
                        }
                        NotePlace::Over(a, b) => {
                            let (a, b) = (self.col(a), self.col(b));
                            (a.min(b), a.max(b), OVER, OVER)
                        }
                        NotePlace::Across => (0, n - 1, OVER, OVER),
                        NotePlace::Message(_) => continue,
                    };
                    let new = Reach {
                        lo,
                        hi,
                        left,
                        right,
                    };
                    for old in &row {
                        if old.hi < new.lo {
                            space.need(old.hi, new.lo, old.right + new.left + NOTE_GAP);
                        } else if new.hi < old.lo {
                            space.need(new.hi, old.lo, new.right + old.left + NOTE_GAP);
                        }
                    }
                    row.push(new);
                }
                Item::Message(_) if self.beside[k] => {}
                Item::Activate { .. } | Item::Deactivate { .. } | Item::Destroy { .. } => {}
                _ => row.clear(),
            }
        }

        self.min_width = min_width;
        for &(a, b, d) in &self.extra.need {
            space.need(a, b, d);
        }
        for &(c, d) in &self.extra.left {
            space.left[c] = space.left[c].max(d);
        }
        for &(c, d) in &self.extra.right {
            space.right[c] = space.right[c].max(d);
        }
        let (x, right) = space.solve();
        self.x = x;
        self.right_edge = right;
    }

    /// Where across the words of a delay `width` wide stand, and whether
    /// clear of the lifelines: in the middle of the room between two
    /// lifelines that holds them [`DELAY_ROOM`] from either and its bars,
    /// the one nearest the picture's middle; else in the middle, over the
    /// lifelines.
    fn delay_x(&self, width: f32) -> (f32, bool) {
        let middle = (self.x[0] + self.x[self.x.len() - 1]) / 2.0;
        let alive: Vec<usize> = (0..self.x.len())
            .filter(|&c| self.head_drawn[c] && self.life_end[c].is_none())
            .collect();
        alive
            .windows(2)
            .filter_map(|pair| {
                let from = self.bar_edge(pair[0], Side::Right, 0);
                let to = self.bar_edge(pair[1], Side::Left, 0);
                (to - from >= width + 2.0 * DELAY_ROOM - 0.01).then_some((from + to) / 2.0)
            })
            .min_by(|a, b| (a - middle).abs().total_cmp(&(b - middle).abs()))
            .map_or((middle, false), |x| (x, true))
    }

    /// The room this drawing found the columns lack ([`room`]).
    fn crowded(&self) -> room::Extra {
        room::crowded(
            &self.drawn,
            &room::Lives {
                x: &self.x,
                bars: &self.bars,
                start: &self.life_start,
                end: &self.life_end,
                bottom: self.bottom,
                right_edge: self.right_edge,
            },
        )
    }

    /// The edge of `column`'s activation bars facing `side`; `extra` bars
    /// that are about to open count too.
    fn bar_edge(&self, column: usize, side: Side, extra: usize) -> f32 {
        let depth = self.open_bars[column].len() + extra;
        let x = self.x[column];
        match (depth, side) {
            (0, _) => x,
            (_, Side::Left) => x - BAR / 2.0,
            (d, Side::Right) => x + BAR / 2.0 + BAR_SHIFT * (d - 1) as f32,
        }
    }

    /// Every open frame holds the lifelines `cols`, and a message from or
    /// to the picture's left or right edge where `edge` says so.
    fn hold(&mut self, cols: &[usize], edge: [bool; 2]) {
        for g in &mut self.groups {
            for &c in cols {
                g.cols = Some(match g.cols {
                    Some((lo, hi)) => (lo.min(c), hi.max(c)),
                    None => (c, c),
                });
            }
            g.edge[0] |= edge[0];
            g.edge[1] |= edge[1];
        }
    }

    /// Widens every open frame and `rect` to hold `lo..hi`.
    fn touch(&mut self, lo: f32, hi: f32) {
        for g in &mut self.groups {
            g.lo = g.lo.min(lo);
            g.hi = g.hi.max(hi);
        }
        for r in &mut self.open_rects {
            r.lo = r.lo.min(lo);
            r.hi = r.hi.max(hi);
        }
    }

    fn run(&mut self, canvas: &mut Canvas) {
        let skin = self.skin;
        let mut top = self.annotations.top();
        let content_top = top;
        let boxes_top = top;
        top += self.box_bands().last().copied().unwrap_or(0.0);
        let heads = self
            .cols
            .iter()
            .filter(|c| c.created_by.is_none())
            .map(|c| c.height)
            .fold(0.0, f32::max);
        for i in 0..self.cols.len() {
            if self.cols[i].created_by.is_none() {
                let head_top = top + heads - self.cols[i].height;
                let (label, fill) = self.head(canvas, i, head_top, false);
                let col = &self.cols[i];
                self.heads.push(crate::map::Heading {
                    rect: crate::graph::Rect {
                        x: self.x[i] - col.half(),
                        y: head_top,
                        w: col.width,
                        h: col.height,
                    },
                    label,
                    line: col
                        .label
                        .lines
                        .first()
                        .map_or(col.label.height, |line| line.height),
                    fill: fill.map(|fill| canvas.color(fill)),
                    border: canvas.color(col.look.border),
                });
                self.life_start[i] = top + heads;
                self.head_drawn[i] = true;
            }
        }
        self.y = top + heads + 3.0;
        let (mut left, mut right) = (f32::MAX, f32::MIN);
        for i in 0..self.cols.len() {
            left = left.min(self.x[i] - self.cols[i].half() - self.pad_left[i]);
            right = right.max(self.x[i] + self.cols[i].half() + self.pad_right[i]);
        }
        let heads_band = (heads > 0.0 && left < right).then_some(crate::graph::Rect {
            x: left,
            y: boxes_top,
            w: right - left,
            h: top + heads - boxes_top,
        });

        let mut k = 0;
        while k < self.d.items.len() {
            k = self.item(canvas, k);
        }

        // Unclosed bars run to the end.
        let end = self.y + 10.0;
        self.bottom = end;
        // The heads stay over the lifelines, not over the feet below them.
        self.band = heads_band.map(|rect| crate::map::Band {
            rect,
            edge: crate::map::Edge::Top,
            // Over the first page: the next ones have heads of their own.
            over: crate::graph::Rect {
                x: rect.x,
                y: rect.y + rect.h,
                w: rect.w,
                h: (self.pages.first().map_or(end, |p| p.end) - rect.y - rect.h).max(0.0),
            },
        });
        for (column, open) in self.open_bars.iter_mut().enumerate() {
            for (depth, (start, fill, line)) in open.drain(..).enumerate() {
                self.bars.push(Bar {
                    column,
                    depth,
                    start,
                    end,
                    fill,
                    line,
                });
            }
        }
        self.lifelines(canvas, end);
        self.bars.sort_by_key(|b| b.depth);
        for bar in &self.bars {
            let x = self.x[bar.column] - BAR / 2.0 + BAR_SHIFT * bar.depth as f32;
            let rect = (x, bar.start, BAR, (bar.end - bar.start).max(4.0));
            // A bar is its participant's.
            canvas.part(&self.column_names[bar.column], |canvas| {
                canvas.rect(
                    Layer::Bars,
                    rect,
                    0.0,
                    bar.fill,
                    Some(Stroke::new(skin.lifeline, 1.0)),
                );
                canvas.shape_of(bar.line, rect);
            });
        }
        let mut bottom = end;
        if self.d.footbox {
            let mut tallest: f32 = 0.0;
            for i in 0..self.cols.len() {
                if self.life_end[i].is_none() && self.head_drawn[i] {
                    self.head(canvas, i, end, true);
                    tallest = tallest.max(self.cols[i].height);
                }
            }
            bottom += tallest;
        }
        // A box on each page, from its top to under its feet; its title
        // on a page after the first is its words again.
        let mut pages = Vec::with_capacity(self.pages.len() + 1);
        let mut page_top = boxes_top;
        for p in &self.pages {
            pages.push((page_top, p.below));
            page_top = p.top;
        }
        pages.push((page_top, bottom));
        let bands = self.box_bands();
        let mut grounds = Vec::new();
        for b in &self.boxes {
            let part = self.names.group_box(b.of);
            // Inside another, it keeps the padding of those around it that
            // end at the same column clear, and starts under their titles.
            let (inset_left, inset_right) = inset(&self.boxes, b, skin.box_padding);
            let left =
                self.x[b.first] - self.cols[b.first].half() - (self.pad_left[b.first] - inset_left);
            let right =
                self.x[b.last] + self.cols[b.last].half() + (self.pad_right[b.last] - inset_right);
            let offset = bands.get(b.depth).copied().unwrap_or(0.0);
            for (page, &(top, under)) in pages.iter().enumerate() {
                let top = top + offset;
                let rect = (left, top, right - left, under + 5.0 - top);
                canvas.part(part, |canvas| {
                    canvas.rect(
                        Layer::Background,
                        rect,
                        skin.round,
                        b.look.fill,
                        Some(Stroke::new(b.look.border, b.look.thickness)),
                    );
                });
                if let Some(fill) = b.look.fill {
                    grounds.push((
                        b.depth,
                        crate::map::Ground {
                            rect: crate::graph::Rect {
                                x: rect.0,
                                y: rect.1,
                                w: rect.2,
                                h: rect.3,
                            },
                            // As the canvas paints it: gray in monochrome.
                            fill: if skin.monochrome { fill.gray() } else { fill },
                        },
                    ));
                }
                let before = canvas.echo(page > 0);
                canvas.part(part, |canvas| {
                    canvas.text(
                        Layer::Background,
                        &b.title,
                        ((left + right) / 2.0, top + 4.0),
                        Align::Center,
                    );
                });
                canvas.echo(before);
            }
        }
        // An inner box first: what looks for the ground under a point
        // finds the box it stands in, not the one around that.
        grounds.sort_by_key(|(depth, _)| std::cmp::Reverse(*depth));
        self.grounds
            .extend(grounds.into_iter().map(|(_, ground)| ground));
        // After the boxes: both sit on the background layer, and a box is
        // a full-height fill. Later in that layer paints on top, still
        // under the lifelines. Outer highlights were closed last, so they
        // are drawn first and an inner one covers them.
        self.paint_highlights(canvas);

        // What spans the whole width.
        let (mut left, mut right) = canvas.width_span();
        right = right.max(left + self.min_width).max(self.right_edge);
        left = left.min(0.0);
        canvas.include(left, content_top);
        canvas.include(right, content_top);
        self.annotations.draw(canvas);
        let dividers = std::mem::take(&mut self.dividers);
        for (cy, label, line, part) in dividers {
            canvas.tie(Some(line));
            canvas.part(&part, |canvas| {
                self.divider(canvas, cy, &label, left, right)
            });
        }
        canvas.tie(None);
    }

    /// Draws the item at `k` (and the note beside it); the next item.
    fn item(&mut self, canvas: &mut Canvas, k: usize) -> usize {
        let d = self.d;
        let item = &d.items[k];
        // A duration is drawn over what came before: the row goes on. In
        // PlantUML a note leaves the message before it the last arrow: a
        // bar opened or a participant destroyed after the note starts or
        // ends at that message ([`Diagram::through_notes`]).
        let passes = match item {
            Item::Activate { .. }
            | Item::Deactivate { .. }
            | Item::Destroy { .. }
            | Item::Duration { .. } => true,
            Item::Note(_) => d.through_notes,
            _ => false,
        };
        if !passes {
            self.last_arrow = None;
            if !matches!(item, Item::Message(_) | Item::Note(_)) {
                self.level = None;
            }
        }
        // What this line alone draws is tied to it; a group's words are
        // drawn where it ends, and tied there.
        canvas.tie(match item {
            Item::Message(m) => Some(m.line),
            Item::Note(note) => Some(note.text.line),
            Item::Delay { line, .. } => Some(*line),
            Item::Ref { text, .. } => Some(text.line),
            Item::NewPage { line, .. } | Item::Duration { line, .. } => Some(*line),
            _ => None,
        });
        // In its element: a group's frame is drawn where it ends.
        let part = match item {
            Item::GroupEnd { .. } => self.groups.last().map(|g| g.part.clone()),
            // The cross that ends a lifeline is its participant's.
            Item::Destroy { participant, .. } => {
                Some(self.names.participant(*participant).to_owned())
            }
            _ => Some(self.names.item(k).to_owned()),
        }
        .unwrap_or_default();
        let next = canvas.part(&part, |canvas| self.item_of(canvas, k));
        canvas.tie(None);
        next
    }

    fn item_of(&mut self, canvas: &mut Canvas, k: usize) -> usize {
        let d = self.d;
        let item = &d.items[k];
        match item {
            Item::Message(m) => {
                let note = match d.items.get(k + 1) {
                    Some(Item::Note(note)) if matches!(note.place, NotePlace::Message(_)) => {
                        Some(note)
                    }
                    _ => None,
                };
                let below = self.share(k, self.beside[k]);
                self.message(canvas, k, m, note);
                self.y = self.y.max(below.unwrap_or(self.y));
                return k + if note.is_some() { 2 } else { 1 };
            }
            Item::Note(note) => {
                let below = self.share(k, self.beside[k]);
                self.note(canvas, k, note);
                self.y = self.y.max(below.unwrap_or(self.y));
            }
            Item::GroupStart { line, .. } => {
                if let Measured::Group {
                    header,
                    label,
                    tab: tab_fill,
                    fill,
                } = &self.measured[k]
                {
                    let (header, label, tab_fill, fill) =
                        (header.clone(), label.clone(), *tab_fill, *fill);
                    let top = self.y + 7.0;
                    let tab = header.height + 4.0;
                    self.y = top + tab.max(label.height + 4.0);
                    self.groups.push(OpenGroup {
                        top,
                        header,
                        label,
                        tab: tab_fill,
                        fill,
                        line: *line,
                        part: self.names.item(k).to_owned(),
                        elses: Vec::new(),
                        else_fills: Vec::new(),
                        lo: f32::INFINITY,
                        hi: f32::NEG_INFINITY,
                        cols: None,
                        edge: [false; 2],
                    });
                }
            }
            Item::GroupElse { line, .. } => {
                if let Measured::Else { label, fill } = &self.measured[k] {
                    let (label, fill) = (label.clone(), *fill);
                    let y = self.y + 7.0;
                    self.y = y + if label.is_empty() {
                        3.0
                    } else {
                        label.height + 2.0
                    };
                    if let Some(g) = self.groups.last_mut() {
                        g.elses.push((y, label, *line));
                        g.else_fills.push(fill);
                    }
                }
            }
            Item::GroupEnd { .. } => self.group_end(canvas),
            Item::Divider { label: written } => {
                if let Measured::Label(label) = &self.measured[k] {
                    let label = label.clone();
                    let h = label.height + 6.0;
                    let cy = self.y + 10.0 + h / 2.0;
                    self.dividers
                        .push((cy, label, written.line, self.names.item(k).to_owned()));
                    self.y = cy + h / 2.0 + 7.0;
                }
            }
            Item::Delay { .. } => {
                if let Measured::Label(label) = &self.measured[k] {
                    let label = label.clone();
                    let start = self.y + 5.0;
                    let end = start + 20.0 + label.height;
                    let (x, clear) = self.delay_x(label.width);
                    canvas.text(Layer::Front, &label, (x, start + 10.0), Align::Center);
                    // Over the lifelines, the dotted lines stop short of them;
                    // the room between two is found by drawing ([`room`]).
                    // A delay without words leaves its lifelines whole.
                    let words = (!clear && !label.is_empty()).then_some([
                        x - label.width / 2.0 - DELAY_ROOM,
                        start + 10.0 - DELAY_ROOM / 2.0,
                        x + label.width / 2.0 + DELAY_ROOM,
                        start + 10.0 + label.height + DELAY_ROOM / 2.0,
                    ]);
                    if let Some([left, top, right, bottom]) = words {
                        self.drawn.delays.push(room::Delay {
                            width: right - left,
                            top,
                            bottom,
                        });
                    }
                    self.delays.push((start, end, words));
                    self.y = end;
                }
            }
            Item::Space { height, .. } => self.y += height.unwrap_or(20.0),
            Item::Ref { over, .. } => self.reference(canvas, k, over),
            Item::Activate {
                participant, line, ..
            } => {
                let c = self.col(*participant);
                let start = self.lifecycle_y(c);
                let fill = match self.measured[k] {
                    Measured::Activate { fill } => fill,
                    _ => self.skin.activation_fill,
                };
                self.open_bars[c].push((start, fill, *line));
            }
            Item::Deactivate { participant, .. } => {
                let c = self.col(*participant);
                let end = self.lifecycle_y(c);
                if let Some((start, fill, line)) = self.open_bars[c].pop() {
                    let depth = self.open_bars[c].len();
                    self.bars.push(Bar {
                        column: c,
                        depth,
                        start,
                        end,
                        fill,
                        line,
                    });
                }
            }
            Item::RectStart { .. } => {
                let fill = match self.measured[k] {
                    Measured::Rect { fill } => fill,
                    _ => None,
                };
                self.open_rects.push(OpenRect {
                    top: self.y,
                    fill,
                    lo: f32::INFINITY,
                    hi: f32::NEG_INFINITY,
                });
            }
            Item::RectEnd { .. } => {
                if let Some(open) = self.open_rects.pop() {
                    let bottom = self.y;
                    let held = open.lo <= open.hi;
                    let (left, right) = if held {
                        (open.lo - RECT_PAD, open.hi + RECT_PAD)
                    } else {
                        let last = self.x.len() - 1;
                        (
                            self.x[0] - self.cols[0].half() - RECT_PAD,
                            self.x[last] + self.cols[last].half() + RECT_PAD,
                        )
                    };
                    self.highlights.push(Highlight {
                        top: open.top,
                        bottom,
                        left,
                        right,
                        fill: open.fill,
                    });
                    // What holds it holds it all; one that holds nothing
                    // widens no frame around it to the participants.
                    if held {
                        self.touch(left, right);
                    }
                    self.y = bottom + 4.0;
                }
            }
            Item::Destroy { participant, .. } => {
                let c = self.col(*participant);
                let at = match self.last_arrow {
                    Some((y, cols)) if cols.contains(&Some(c)) => y,
                    _ => {
                        self.y += 12.0;
                        self.y
                    }
                };
                let x = self.x[c];
                let stroke = Stroke::new(self.skin.destroy, 2.0);
                canvas.line(
                    Layer::Front,
                    (x - 9.0, at - 9.0),
                    (x + 9.0, at + 9.0),
                    stroke,
                );
                canvas.line(
                    Layer::Front,
                    (x - 9.0, at + 9.0),
                    (x + 9.0, at - 9.0),
                    stroke,
                );
                for (depth, (start, fill, line)) in self.open_bars[c].drain(..).enumerate() {
                    self.bars.push(Bar {
                        column: c,
                        depth,
                        start,
                        end: at,
                        fill,
                        line,
                    });
                }
                if self.life_end[c].is_none() {
                    self.life_end[c] = Some(at);
                }
                self.y = self.y.max(at + 12.0);
            }
            Item::NewPage { .. } => self.new_page(canvas, k),
            Item::Duration { from, to, .. } => self.duration(canvas, k, from, to),
        }
        k + 1
    }

    /// From the top of a page's boxes down: where the boxes of each depth
    /// start, under the titles of those around them, and last where the
    /// heads start. Empty without boxes.
    fn box_bands(&self) -> Vec<f32> {
        if self.boxes.is_empty() {
            return Vec::new();
        }
        let deepest = self.boxes.iter().map(|b| b.depth).max().unwrap_or(0);
        let mut bands = vec![0.0];
        for depth in 0..=deepest {
            let title = self
                .boxes
                .iter()
                .filter(|b| b.depth == depth)
                .map(|b| b.title.height)
                .fold(0.0, f32::max);
            let below = bands.last().copied().unwrap_or(0.0);
            bands.push(below + title + if title > 0.0 { 8.0 } else { 5.0 });
        }
        bands
    }

    /// Teoz's `{start} <-> {end} : words`: an arrow up and down from the
    /// height of one anchored message to the other's, at the middle of the
    /// first, its words beside it. It takes no room of its own.
    fn duration(&mut self, canvas: &mut Canvas, k: usize, from: &str, to: &str) {
        let (Some(&(y0, x)), Some(&(y1, _))) = (self.anchors.get(from), self.anchors.get(to))
        else {
            return;
        };
        let label = match &self.measured[k] {
            Measured::Label(label) => label.clone(),
            _ => Block::empty(),
        };
        let stroke = Stroke::new(self.skin.arrow, self.skin.arrow_thickness);
        let (top, bottom) = (y0.min(y1), y0.max(y1));
        // A piece on each page it crosses, as Teoz draws one: the head up
        // on the first, the head down on the last.
        let mut pieces = Vec::new();
        let mut from = top;
        for p in self
            .pages
            .iter()
            .filter(|p| p.end > top && p.resume < bottom)
        {
            pieces.push((from, p.end));
            from = p.resume;
        }
        pieces.push((from, bottom));
        for &(a, b) in &pieces {
            canvas.line(Layer::Front, (x, a), (x, b), stroke);
        }
        // A head drawn pointing right, turned down and up.
        for (tip, turn) in [((x, bottom), (0.0, 1.0)), ((x, top), (0.0, -1.0))] {
            self.arrow_end(
                canvas,
                tip,
                1.0,
                Head::Filled,
                Mark::None,
                stroke,
                Some(turn),
            );
        }
        // The words beside the longest piece, where they have room.
        let (a, b) = pieces
            .iter()
            .copied()
            .max_by(|p, q| (p.1 - p.0).total_cmp(&(q.1 - q.0)))
            .unwrap_or((top, bottom));
        canvas.text(
            Layer::Front,
            &label,
            (x + 6.0, (a + b - label.height) / 2.0),
            Align::Left,
        );
    }

    /// `newpage`: this page ends, with its feet, and the next starts under
    /// it with its title, its boxes and the heads again; the bars open now
    /// go on.
    fn new_page(&mut self, canvas: &mut Canvas, k: usize) {
        let title = match &self.measured[k] {
            Measured::Label(title) => title.clone(),
            _ => Block::empty(),
        };
        let end = self.y + 10.0;
        let live: Vec<usize> = (0..self.cols.len())
            .filter(|&c| self.head_drawn[c] && self.life_end[c].is_none())
            .collect();
        let mut feet: f32 = 0.0;
        if self.d.footbox {
            for &c in &live {
                self.head(canvas, c, end, true);
                feet = feet.max(self.cols[c].height);
            }
        }
        let below = end + feet;
        let mut top = below + PAGE_GAP;
        // Over the heads alive, or over all of them when none is.
        let spanned = if live.is_empty() {
            (0..self.cols.len()).collect()
        } else {
            live.clone()
        };
        if let (false, Some(&first), Some(&last)) =
            (title.is_empty(), spanned.first(), spanned.last())
        {
            let left = self.x[first] - self.cols[first].half();
            let right = self.x[last] + self.cols[last].half();
            canvas.text(
                Layer::Front,
                &title,
                ((left + right) / 2.0, top),
                Align::Center,
            );
            top += title.height + 8.0;
        }
        let boxes_top = top;
        top += self.box_bands().last().copied().unwrap_or(0.0);
        let heads = live
            .iter()
            .map(|&c| self.cols[c].height)
            .fold(0.0, f32::max);
        for &c in &live {
            let head_top = top + heads - self.cols[c].height;
            self.head_drawn_again(canvas, c, head_top, false, true);
        }
        let resume = top + heads;
        for (column, open) in self.open_bars.iter_mut().enumerate() {
            for (depth, (start, fill, line)) in open.iter_mut().enumerate() {
                self.bars.push(Bar {
                    column,
                    depth,
                    start: *start,
                    end,
                    fill: *fill,
                    line: *line,
                });
                *start = resume;
            }
        }
        self.pages.push(PageBreak {
            end,
            below,
            top: boxes_top,
            resume,
        });
        self.y = resume + 3.0;
    }

    /// The `rect` highlights, each as wide as what it holds.
    fn paint_highlights(&self, canvas: &mut Canvas) {
        for band in self.highlights.iter().rev() {
            let (left, right) = (band.left, band.right);
            let Some(fill) = band.fill else {
                continue;
            };
            let height = band.bottom - band.top;
            if height <= 0.0 {
                continue;
            }
            canvas.plain(|canvas| {
                canvas.rect(
                    Layer::Background,
                    (left, band.top, right - left, height),
                    0.0,
                    Some(fill),
                    None,
                );
            });
        }
    }

    /// Where an activation change of column `c` happens: at the arrow of
    /// the message right before it, if that message touched `c`.
    fn lifecycle_y(&self, c: usize) -> f32 {
        match self.last_arrow {
            Some((y, cols)) if cols.contains(&Some(c)) => y,
            _ => self.y,
        }
    }

    fn message(&mut self, canvas: &mut Canvas, k: usize, m: &Message, note: Option<&Note>) {
        let skin = self.skin;
        let d = self.d;
        let (text, number, color) = match &self.measured[k] {
            Measured::Message {
                text,
                number,
                color,
            } => (text.clone(), number.clone(), *color),
            _ => return,
        };
        let (from, to) = (self.end_col(m.from), self.end_col(m.to));
        // Bars the following items open count already: on the receiver,
        // and for the frames around, on the sender.
        let opened = |column: Option<usize>| {
            d.items[k + 1..]
                .iter()
                .enumerate()
                // A note beside the message is drawn with it.
                .skip_while(|(at, i)| {
                    *at == 0
                        && matches!(i, Item::Note(note) if matches!(note.place, NotePlace::Message(_)))
                })
                .map(|(_, i)| i)
                // In PlantUML across the notes after it too, as the bar
                // starts at the message.
                .take_while(|i| match i {
                    Item::Activate { .. } | Item::Deactivate { .. } | Item::Destroy { .. } => true,
                    Item::Note(_) => d.through_notes,
                    _ => false,
                })
                .filter(|i| matches!(i, Item::Activate { participant, .. } if Some(self.col(*participant)) == column))
                .count()
        };
        let opening = opened(to);
        let created = to.filter(|&c| self.cols[c].created_by == Some(k) && !self.head_drawn[c]);

        let text_w = self.message_text_width(k);
        let text_h = text.height.max(number.height);
        let band_top = self.y;
        let points: Vec<(f32, f32)>;
        let top = self.y + 9.0;
        // An answer's words under its arrow, else over it.
        let below = self.words_below(k);
        let mut arrow_y = if below {
            top + 3.0
        } else {
            top + text_h + if text_h > 0.0 { 2.0 } else { 3.0 }
        };
        if let Some(c) = created {
            arrow_y = arrow_y.max(self.y + self.cols[c].height / 2.0 + 4.0);
        }
        // Under the arrow where it ends lowest, a slant or a head made.
        let mut words_top = if below { arrow_y + 3.0 } else { top };
        let mut after = arrow_y + 3.0;

        let stroke = Stroke::new(
            color.unwrap_or(skin.arrow),
            skin.arrow_thickness * if m.arrow.bold { 2.0 } else { 1.0 },
        );
        let stroke = if m.arrow.dashed {
            stroke.dashed(2.0, 2.0)
        } else {
            stroke
        };

        // Where the text goes, where the message runs (its notes stand
        // beside that), and how far the frames around reach for it.
        let (text_left, lo, hi, reach);
        // Where activations and a destruction right after it happen.
        let lifecycle_y;
        if let (Some(a), Some(b)) = (from, to)
            && a == b
        {
            let start = self.bar_edge(a, Side::Right, 0);
            let back = self.bar_edge(a, Side::Right, opening);
            let out = start.max(back) + LOOP_WIDTH;
            let bottom = arrow_y + LOOP_HEIGHT;
            canvas.polyline(
                Layer::Front,
                &[
                    (start, arrow_y),
                    (out, arrow_y),
                    (out, bottom),
                    (back, bottom),
                ],
                stroke,
            );
            self.arrow_end(
                canvas,
                (back, bottom),
                -1.0,
                m.arrow.head,
                m.arrow.end_mark,
                stroke,
                None,
            );
            if m.arrow.tail != Head::None || m.arrow.start_mark != Mark::None {
                self.arrow_end(
                    canvas,
                    (start, arrow_y),
                    -1.0,
                    m.arrow.tail,
                    m.arrow.start_mark,
                    stroke,
                    None,
                );
            }
            points = vec![
                (start, arrow_y),
                (out, arrow_y),
                (out, bottom),
                (back, bottom),
            ];
            text_left = start
                + self
                    .circled(k)
                    .map_or(TEXT_INSET, |keep| keep.max(TEXT_INSET));
            lo = self.x[a] - BAR / 2.0;
            hi = out.max(text_left + text_w);
            reach = (lo, hi);
            after = bottom + 3.0;
            lifecycle_y = bottom;
        } else {
            let place = |end: End, other: Option<usize>, receiving: bool, s: &Self| -> f32 {
                let toward = |c: usize, o: f32| if o >= s.x[c] { Side::Right } else { Side::Left };
                match end {
                    End::Participant(p) => {
                        let c = s.col(p);
                        let other_x = match other {
                            Some(o) => s.x[o],
                            None => match if receiving { m.from } else { m.to } {
                                End::Edge(Side::Left) | End::Short(Side::Left) => f32::NEG_INFINITY,
                                _ => f32::INFINITY,
                            },
                        };
                        if receiving && created == Some(c) {
                            let half = s.cols[c].half();
                            return if other_x < s.x[c] {
                                s.x[c] - half
                            } else {
                                s.x[c] + half
                            };
                        }
                        s.bar_edge(c, toward(c, other_x), if receiving { opening } else { 0 })
                    }
                    End::Edge(Side::Left) => 0.0,
                    End::Edge(Side::Right) => s.right_edge,
                    End::Short(side) => {
                        let c = other.unwrap_or(0);
                        let len = (text_w + MESSAGE_ROOM).max(30.0);
                        match side {
                            Side::Left => s.x[c] - len,
                            Side::Right => s.x[c] + len,
                        }
                    }
                }
            };
            let sx = place(m.from, to, false, self);
            let tx = place(m.to, from, true, self);
            let dir = if tx >= sx { 1.0 } else { -1.0 };
            // A slanted arrow reaches its receiver lower down, and what
            // happens there happens at that height; one with heads at both
            // ends falls toward the right, as PlantUML draws it.
            let low = arrow_y + m.slant;
            let (sy, ty) = if m.arrow.tail != Head::None && tx < sx {
                (low, arrow_y)
            } else {
                (arrow_y, low)
            };
            self.arrow_line(canvas, (sx, sy), (tx, ty), dir, m, stroke);
            points = vec![(sx, sy), (tx, ty)];
            after = after.max(low + 3.0);
            lifecycle_y = low;
            if below {
                words_top = low + 3.0;
            }
            let (l, r) = (sx.min(tx), sx.max(tx));
            // Clear of a head at either end.
            let head_at = |left_end: bool| {
                let (head, mark) = if (dir < 0.0) == left_end {
                    (m.arrow.head, m.arrow.end_mark)
                } else {
                    (m.arrow.tail, m.arrow.start_mark)
                };
                if head != Head::None || mark != Mark::None {
                    HEAD_LENGTH - 3.0
                } else {
                    0.0
                }
            };
            let aligned = match skin.message_align {
                MessageAlign::Left => l + TEXT_INSET + head_at(true),
                MessageAlign::Center => (l + r - text_w) / 2.0,
                MessageAlign::Right => r - TEXT_INSET - head_at(false) - text_w,
            };
            // Clear of the circle its number stands in at the arrow's start.
            text_left = match self.circled(k) {
                Some(keep) if dir > 0.0 => aligned.max(sx + keep),
                Some(keep) => aligned.min(sx - keep - text_w),
                None => aligned,
            };
            let ends: Vec<f32> = [from, to].iter().flatten().map(|&c| self.x[c]).collect();
            lo = ends
                .iter()
                .copied()
                .fold(f32::INFINITY, f32::min)
                .min(text_left);
            hi = ends
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max)
                .max(text_left + text_w);
            // The frames around keep their margin from what stands at either
            // end: the bars on its lifelines, those opened right after it
            // too, and the head of a participant it creates.
            let sending = opened(from);
            let extent = |c: usize| {
                if created == Some(c) {
                    let half = self.cols[c].half();
                    return (self.x[c] - half, self.x[c] + half);
                }
                let extra = if Some(c) == to { opening } else { sending };
                (
                    self.bar_edge(c, Side::Left, extra),
                    self.bar_edge(c, Side::Right, extra),
                )
            };
            reach = [from, to]
                .iter()
                .flatten()
                .map(|&c| extent(c))
                .fold((lo, hi), |(l, h), (a, b)| (l.min(a), h.max(b)));
        }
        if let (Some(anchor), Some(&(sx, sy)), Some(&(tx, _))) =
            (&m.anchor, points.first(), points.last())
        {
            self.anchors.insert(anchor.clone(), (sy, (sx + tx) / 2.0));
        }
        let mut reach = reach;
        if below {
            after = after.max(words_top + text_h + 3.0);
        }
        if self.d.numbers_circled {
            canvas.text(Layer::Front, &text, (text_left, words_top), Align::Left);
            // The number in a circle of the arrows' color at its start,
            // held by its band and by what holds the message.
            if !number.is_empty()
                && let Some(&(x, y)) = points.first()
            {
                let radius = circle_radius(&number);
                reach = (reach.0.min(x - radius), reach.1.max(x + radius));
                after = after.max(y + radius + 2.0);
                canvas.ellipse(
                    Layer::Front,
                    (x, y),
                    (radius, radius),
                    Some(color.unwrap_or(skin.arrow)),
                    None,
                );
                canvas.text(
                    Layer::Front,
                    &number,
                    (x, y - number.height / 2.0),
                    Align::Center,
                );
            }
        } else {
            canvas.text(Layer::Front, &number, (text_left, words_top), Align::Left);
            let gap = if number.is_empty() {
                0.0
            } else {
                number.width + 4.0
            };
            canvas.text(
                Layer::Front,
                &text,
                (text_left + gap, words_top),
                Align::Left,
            );
        }
        self.touch(reach.0, reach.1);
        let ends: Vec<usize> = [from, to].into_iter().flatten().collect();
        let edge = |side: Side| [m.from, m.to].contains(&End::Edge(side));
        self.hold(&ends, [edge(Side::Left), edge(Side::Right)]);

        if let Some(c) = created {
            let head_top = arrow_y - self.cols[c].height / 2.0;
            self.head(canvas, c, head_top, false);
            self.head_drawn[c] = true;
            self.life_start[c] = head_top + self.cols[c].height;
            after = after.max(head_top + self.cols[c].height + 3.0);
        }

        // The note beside it, lit with it.
        let mut also = Vec::new();
        if let Some(note) = note
            && let Some(Measured::Note {
                text: note_text,
                width,
                height,
                fill,
            }) = self.measured.get(k + 1)
            && let NotePlace::Message(side) = note.place
        {
            // The note's words are its own line's, not the message's.
            canvas.tie(Some(note.text.line));
            let (width, height) = (*width, *height);
            // Beside the message and the bars at its ends.
            let nx = match side {
                Side::Left => reach.0 - 5.0 - width,
                Side::Right => reach.1 + 5.0,
            };
            let ny = self.y + 7.0;
            let ends = [from, to].into_iter().flatten();
            let column = match side {
                Side::Left => ends.min_by(|a, b| self.x[*a].total_cmp(&self.x[*b])),
                Side::Right => ends.max_by(|a, b| self.x[*a].total_cmp(&self.x[*b])),
            };
            if let Some(column) = column {
                self.drawn.notes.push(room::Note {
                    column,
                    right: side == Side::Right,
                    at: [nx, ny, nx + width, ny + height],
                });
            }
            // In a part of its own within the message's.
            let part = self.names.item(k + 1).to_owned();
            canvas.part(&part, |canvas| {
                draw_note(
                    canvas,
                    skin,
                    note.shape,
                    (nx, ny, width, height),
                    note_text,
                    *fill,
                );
            });
            also.push([nx, ny, width, height]);
            self.touch(nx, nx + width);
            after = after.max(ny + height + 3.0);
        }
        self.y = after;
        self.last_arrow = Some((lifecycle_y, [from, to]));
        self.steps.push(Step {
            line: m.line,
            points,
            top: band_top,
            bottom: after,
            also,
        });
    }

    /// The arrow of a message from `(sx, sy)` to its tip at `(tx, ty)`,
    /// `dir` 1 to the right, -1 to the left; one that slants (`->(10)`)
    /// has its heads and marks turned along it.
    fn arrow_line(
        &self,
        canvas: &mut Canvas,
        (sx, sy): (f32, f32),
        (tx, ty): (f32, f32),
        dir: f32,
        m: &Message,
        stroke: Stroke,
    ) {
        let a = &m.arrow;
        // Along the arrow toward its tip, and how far that turns a head
        // drawn level.
        let (ux, uy, turn) = if ty == sy {
            (dir, 0.0, None)
        } else {
            let (dx, dy) = (tx - sx, ty - sy);
            let length = dx.hypot(dy);
            let (ux, uy) = (dx / length, dy / length);
            (ux, uy, Some((ux * dir, uy * dir)))
        };
        let mut start = (sx, sy);
        let mut end = (tx, ty);
        // An `o` is a dot in the arrow's color, as PlantUML draws it.
        let circle = Stroke {
            dash: None,
            ..stroke
        };
        let dot = Some(stroke.color);
        match a.end_mark {
            Mark::Circle => {
                canvas.ellipse(
                    Layer::Front,
                    (tx - ux * 4.0, ty - uy * 4.0),
                    (4.0, 4.0),
                    dot,
                    Some(circle),
                );
                end = (tx - ux * 8.0, ty - uy * 8.0);
            }
            Mark::Cross => {}
            Mark::None => {}
        }
        match a.start_mark {
            Mark::Circle => {
                canvas.ellipse(
                    Layer::Front,
                    (sx + ux * 4.0, sy + uy * 4.0),
                    (4.0, 4.0),
                    dot,
                    Some(circle),
                );
                start = (sx + ux * 8.0, sy + uy * 8.0);
            }
            Mark::Cross => cross(canvas, (sx, sy), stroke),
            Mark::None => {}
        }
        canvas.line(Layer::Front, start, end, stroke);
        if a.end_mark == Mark::Cross {
            cross(canvas, (tx, ty), stroke);
        } else {
            self.arrow_end(canvas, end, dir, a.head, Mark::None, stroke, turn);
        }
        if a.tail != Head::None {
            self.arrow_end(canvas, start, -dir, a.tail, Mark::None, stroke, turn);
        }
    }

    /// A head with its tip at `tip`, pointing in `dir` (1 right, -1 left),
    /// and turned about its tip by `turn` (the cosine and sine) where its
    /// arrow slants.
    #[allow(clippy::too_many_arguments)]
    fn arrow_end(
        &self,
        canvas: &mut Canvas,
        (x, y): (f32, f32),
        dir: f32,
        head: Head,
        mark: Mark,
        stroke: Stroke,
        turn: Option<(f32, f32)>,
    ) {
        if mark == Mark::Cross {
            cross(canvas, (x, y), stroke);
            return;
        }
        // A dot at the tip (a self message's `->o`), the head behind it.
        if mark == Mark::Circle {
            let ring = Stroke {
                dash: None,
                ..stroke
            };
            canvas.ellipse(
                Layer::Front,
                (x - dir * 4.0, y),
                (4.0, 4.0),
                Some(stroke.color),
                Some(ring),
            );
            self.arrow_end(
                canvas,
                (x - dir * 8.0, y),
                dir,
                head,
                Mark::None,
                stroke,
                turn,
            );
            return;
        }
        let solid = Stroke {
            dash: None,
            width: stroke.width.min(1.5),
            ..stroke
        };
        let at = |px: f32, py: f32| match turn {
            None => (px, py),
            Some((cos, sin)) => (
                x + cos * (px - x) - sin * (py - y),
                y + sin * (px - x) + cos * (py - y),
            ),
        };
        let back = x - dir * HEAD_LENGTH;
        let fill = Some(stroke.color);
        match head {
            Head::None => {}
            Head::Filled if self.skin.strict => canvas.polygon(
                Layer::Front,
                &[at(back, y - HEAD_HALF), (x, y), at(back, y + HEAD_HALF)],
                fill,
                Some(solid),
            ),
            Head::Filled => canvas.polygon(
                Layer::Front,
                &[
                    at(back, y - HEAD_HALF),
                    (x, y),
                    at(back, y + HEAD_HALF),
                    at(x - dir * 6.0, y),
                ],
                fill,
                Some(solid),
            ),
            Head::Open => canvas.polyline(
                Layer::Front,
                &[at(back, y - HEAD_HALF), (x, y), at(back, y + HEAD_HALF)],
                solid,
            ),
            Head::UpperFilled => canvas.polygon(
                Layer::Front,
                &[at(back, y - HEAD_HALF), (x, y), at(back, y)],
                fill,
                Some(solid),
            ),
            Head::LowerFilled => canvas.polygon(
                Layer::Front,
                &[at(back, y + HEAD_HALF), (x, y), at(back, y)],
                fill,
                Some(solid),
            ),
            Head::UpperOpen => canvas.line(Layer::Front, at(back, y - HEAD_HALF), (x, y), solid),
            Head::LowerOpen => canvas.line(Layer::Front, at(back, y + HEAD_HALF), (x, y), solid),
        }
    }

    fn note(&mut self, canvas: &mut Canvas, k: usize, note: &Note) {
        let (text, mut width, height, fill) = match &self.measured[k] {
            Measured::Note {
                text,
                width,
                height,
                fill,
            } => (text.clone(), *width, *height, *fill),
            _ => return,
        };
        let last = self.cols.len() - 1;
        // The lifeline a note stands beside, and on which side.
        let mut beside: Option<(usize, bool)> = None;
        let held: Vec<usize> = match note.place {
            NotePlace::LeftOf(p) | NotePlace::RightOf(p) => vec![self.col(p)],
            NotePlace::Over(a, b) => {
                let (a, b) = (self.col(a), self.col(b));
                (a.min(b)..=a.max(b)).collect()
            }
            NotePlace::Across => (0..=last).collect(),
            NotePlace::Message(_) => Vec::new(),
        };
        let x = match note.place {
            NotePlace::LeftOf(p) => {
                let c = self.col(p);
                beside = Some((c, false));
                self.bar_edge(c, Side::Left, 0) - 5.0 - width
            }
            NotePlace::RightOf(p) => {
                let c = self.col(p);
                beside = Some((c, true));
                self.bar_edge(c, Side::Right, 0) + 5.0
            }
            NotePlace::Over(a, b) => {
                let (a, b) = (self.col(a), self.col(b));
                let (lo, hi) = (a.min(b), a.max(b));
                let (x, w) = over_span(&self.x, lo, hi, width);
                width = w;
                x
            }
            NotePlace::Across => {
                let (x, w) = over_span(&self.x, 0, last, width);
                width = w;
                x
            }
            // Beside a message, drawn with it.
            NotePlace::Message(_) => return,
        };
        let top = self.y;
        let y = self.y + 7.0;
        draw_note(
            canvas,
            self.skin,
            note.shape,
            (x, y, width, height),
            &text,
            fill,
        );
        // A step of a play of its own, lit as a message is (REQ-106). One
        // beside what stands before it (`/`) reaches at least as far down,
        // so the veil does not climb back over it.
        let before = self
            .steps
            .iter()
            .chain(&self.note_steps)
            .map(|step| step.bottom)
            .fold(0.0, f32::max);
        self.note_steps.push(Step {
            line: note.line,
            points: Vec::new(),
            top,
            bottom: if note.together {
                (y + height + 3.0).max(before)
            } else {
                y + height + 3.0
            },
            also: vec![[x, y, width, height]],
        });
        if let Some((column, right)) = beside {
            self.drawn.notes.push(room::Note {
                column,
                right,
                at: [x, y, x + width, y + height],
            });
        }
        self.touch(x, x + width);
        self.hold(&held, [false; 2]);
        self.y = y + height + 3.0;
    }

    fn reference(&mut self, canvas: &mut Canvas, k: usize, list: &[usize]) {
        let skin = self.skin;
        let (text, header, fill) = match &self.measured[k] {
            Measured::Ref { text, header, fill } => (text.clone(), header.clone(), *fill),
            _ => return,
        };
        let cols: Vec<usize> = list.iter().map(|&p| self.col(p)).collect();
        let lo = cols.iter().copied().min().unwrap_or(0);
        let hi = cols.iter().copied().max().unwrap_or(0);
        let wanted = (text.width + 20.0).max(header.width + 30.0);
        let (x, w) = over_span(&self.x, lo, hi, wanted);
        let top = self.y + 7.0;
        let tab_h = header.height + 4.0;
        let h = tab_h + text.height + 10.0;
        let border = Stroke::new(skin.reference.border, skin.reference.thickness);
        canvas.rect(Layer::Front, (x, top, w, h), skin.round, fill, Some(border));
        let tab_w = header.width + 18.0;
        canvas.polygon(
            Layer::Front,
            &tab(x, top, tab_w, tab_h),
            skin.reference_header_fill,
            Some(border),
        );
        canvas.text(Layer::Front, &header, (x + 6.0, top + 2.0), Align::Left);
        canvas.text(
            Layer::Front,
            &text,
            (x + w / 2.0, top + tab_h + 5.0),
            Align::Center,
        );
        self.touch(x, x + w);
        self.hold(&(lo..=hi).collect::<Vec<_>>(), [false; 2]);
        self.y = top + h + 3.0;
    }

    fn group_end(&mut self, canvas: &mut Canvas) {
        let Some(g) = self.groups.pop() else {
            return;
        };
        let skin = self.skin;
        let bottom = self.y + 7.0;
        let (lo, hi) = if g.lo <= g.hi {
            (g.lo, g.hi)
        } else {
            (self.x[0], self.x[self.x.len() - 1])
        };
        let tab_w = g.header.width + 18.0;
        let tab_h = g.header.height + 4.0;
        // A message from the left edge enters it there.
        let left = if g.edge[0] {
            (lo - FRAME).max(0.0)
        } else {
            lo - FRAME
        };
        let mut right = (hi + FRAME).max(left + tab_w + 10.0);
        if !g.label.is_empty() {
            right = right.max(left + tab_w + 5.0 + g.label.width + 10.0);
        }
        for (_, label, _) in &g.elses {
            right = right.max(left + label.width + 15.0);
        }
        // A message to the right edge leaves it there.
        if g.edge[1] {
            right = right.min(self.right_edge).max(left + tab_w + 10.0);
        }
        let border = Stroke::new(skin.group_border, skin.group_thickness);
        canvas.rect(
            Layer::Frames,
            (left, g.top, right - left, bottom - g.top),
            0.0,
            None,
            Some(border),
        );
        // The tab and the labels above activation bars.
        canvas.polygon(
            Layer::Front,
            &tab(left, g.top, tab_w, tab_h),
            g.tab,
            Some(border),
        );
        // Each part's color behind it (`alt #LightBlue`, `else #Pink`),
        // painted with the highlights, under the lifelines.
        let tops = std::iter::once(g.top).chain(g.elses.iter().map(|(y, _, _)| *y));
        let bottoms = g
            .elses
            .iter()
            .map(|(y, _, _)| *y)
            .chain(std::iter::once(bottom));
        let fills = std::iter::once(g.fill).chain(g.else_fills.iter().copied());
        for ((top, end), fill) in tops.zip(bottoms).zip(fills) {
            if fill.is_some() {
                self.highlights.push(Highlight {
                    top,
                    bottom: end,
                    left,
                    right,
                    fill,
                });
            }
        }
        canvas.tie(Some(g.line));
        canvas.text(
            Layer::Front,
            &g.header,
            (left + 6.0, g.top + 2.0),
            Align::Left,
        );
        canvas.text(
            Layer::Front,
            &g.label,
            (
                left + tab_w + 5.0,
                g.top + 2.0 + (g.header.height - g.label.height).max(0.0) / 2.0,
            ),
            Align::Left,
        );
        for (y, label, line) in &g.elses {
            canvas.line(
                Layer::Frames,
                (left, *y),
                (right, *y),
                Stroke::new(skin.group_border, 1.0).dashed(2.0, 2.0),
            );
            canvas.tie(Some(*line));
            canvas.text(Layer::Front, label, (left + 5.0, *y + 1.0), Align::Left);
        }
        canvas.tie(None);
        let mut words = Vec::new();
        if !g.label.is_empty() {
            let from = left + tab_w + 5.0;
            words.push([
                from,
                g.top + 2.0,
                from + g.label.width,
                g.top + 2.0 + g.label.height,
            ]);
        }
        for (y, label, _) in &g.elses {
            if !label.is_empty() {
                words.push([
                    left + 5.0,
                    *y + 1.0,
                    left + 5.0 + label.width,
                    *y + 1.0 + label.height,
                ]);
            }
        }
        self.drawn.frames.push(room::Frame {
            left,
            right,
            top: g.top,
            bottom,
            words,
            held: g.cols,
            edge: g.edge,
        });
        self.y = bottom;
        self.touch(left, right);
        if let Some((lo, hi)) = g.cols {
            self.hold(&[lo, hi], g.edge);
        }
    }

    fn divider(&self, canvas: &mut Canvas, cy: f32, label: &Block, left: f32, right: f32) {
        let skin = self.skin;
        let look = &skin.divider;
        canvas.rect(
            Layer::Front,
            (left, cy - 1.5, right - left, 3.0),
            0.0,
            look.fill,
            Some(Stroke::new(look.border, look.thickness)),
        );
        if label.width > 0.0 {
            let w = label.width + 12.0;
            let h = label.height + 6.0;
            let middle = (left + right) / 2.0;
            canvas.rect(
                Layer::Front,
                (middle - w / 2.0, cy - h / 2.0, w, h),
                skin.round,
                look.fill,
                Some(Stroke::new(look.border, 2.0 * look.thickness)),
            );
            canvas.text(
                Layer::Front,
                label,
                (middle, cy - label.height / 2.0),
                Align::Center,
            );
        }
    }

    fn lifelines(&self, canvas: &mut Canvas, end: f32) {
        let skin = self.skin;
        let line = Stroke::new(skin.lifeline, skin.lifeline_thickness);
        let dashed = if skin.lifeline_solid {
            line
        } else {
            line.dashed(5.0, 5.0)
        };
        let dotted = Stroke::new(skin.lifeline, skin.lifeline_thickness.max(1.0)).dashed(1.0, 4.0);
        for c in 0..self.cols.len() {
            if !self.head_drawn[c] {
                continue;
            }
            // A lifeline is its participant's.
            canvas.part(&self.column_names[c], |canvas| {
                self.lifeline(canvas, c, end, (dashed, dotted))
            });
        }
    }

    /// Column `c`'s lifeline, down to `end` but where it ends earlier;
    /// `dashed` where it runs, `dotted` over a delay.
    fn lifeline(
        &self,
        canvas: &mut Canvas,
        c: usize,
        end: f32,
        (dashed, dotted): (Stroke, Stroke),
    ) {
        let x = self.x[c];
        let mut y = self.life_start[c];
        let stop = self.life_end[c].unwrap_or(end);
        // Between pages it does not run; past a page it starts below.
        let mut breaks: Vec<(f32, f32, Break)> = self
            .delays
            .iter()
            .map(|&(d0, d1, words)| (d0, d1, Break::Delay(words)))
            .chain(self.pages.iter().map(|p| (p.end, p.resume, Break::Page)))
            .collect();
        breaks.sort_by(|a, b| a.0.total_cmp(&b.0));
        for (d0, d1, kind) in breaks {
            let Break::Delay(words) = kind else {
                if d0 > y && y < stop {
                    canvas.line(Layer::Lifelines, (x, y), (x, d0.min(stop)), dashed);
                }
                y = y.max(d1);
                continue;
            };
            if d1 <= y || d0 >= stop {
                continue;
            }
            if d0 > y {
                canvas.line(Layer::Lifelines, (x, y), (x, d0), dashed);
            }
            let (from, to) = (d0.max(y), d1.min(stop));
            match words {
                Some([left, top, right, bottom]) if x > left && x < right => {
                    if top > from {
                        canvas.line(Layer::Lifelines, (x, from), (x, top), dotted);
                    }
                    if to > bottom {
                        canvas.line(Layer::Lifelines, (x, bottom), (x, to), dotted);
                    }
                }
                _ => canvas.line(Layer::Lifelines, (x, from), (x, to), dotted),
            }
            y = to;
        }
        if stop > y {
            canvas.line(Layer::Lifelines, (x, y), (x, stop), dashed);
        }
    }

    /// Draws column `c`'s head with its top at `top`; at the bottom the
    /// label of an icon goes above it. A linked head is a link. Gives
    /// where its name stands, and the fill under the name (none under a
    /// figure, where it stands on the paper).
    fn head(
        &self,
        canvas: &mut Canvas,
        c: usize,
        top: f32,
        bottom: bool,
    ) -> (crate::graph::Rect, Paint) {
        self.head_drawn_again(canvas, c, top, bottom, bottom)
    }

    /// [`Self::head_shape_drawn`] in its participant's element.
    fn head_drawn_again(
        &self,
        canvas: &mut Canvas,
        c: usize,
        top: f32,
        bottom: bool,
        again: bool,
    ) -> (crate::graph::Rect, Paint) {
        // Its id on its head, not on the lifeline under it; a linked head
        // is a link, drawn in no part, and its lifeline keeps the id.
        let linked = (0..self.d.participants.len())
            .any(|p| self.col_of[p] == Some(c) && self.d.participants[p].link.is_some());
        if !linked {
            canvas.id_on(&self.column_names[c], Layer::Front);
        }
        canvas.part(&self.column_names[c], |canvas| {
            self.head_shape_drawn(canvas, c, top, bottom, again)
        })
    }

    /// [`Self::head`]; `again` where the name is drawn a second time, at
    /// the foot or on a page after the first.
    fn head_shape_drawn(
        &self,
        canvas: &mut Canvas,
        c: usize,
        top: f32,
        bottom: bool,
        again: bool,
    ) -> (crate::graph::Rect, Paint) {
        // A participant's name is no one line's: many mention it, also
        // where a message's line creates it.
        let line = canvas.tie(None);
        // At the foot, or on a later page, it is the head's name again.
        let before = canvas.echo(again);
        let mut name = (crate::graph::Rect::default(), None);
        canvas.linked(self.cols[c].link.as_ref(), &[Layer::Front], |canvas| {
            name = self.head_shape(canvas, c, top, bottom);
        });
        canvas.echo(before);
        canvas.tie(line);
        name
    }

    fn head_shape(
        &self,
        canvas: &mut Canvas,
        c: usize,
        top: f32,
        bottom: bool,
    ) -> (crate::graph::Rect, Paint) {
        let named = |cx: f32, top: f32, col: &Column| crate::graph::Rect {
            x: cx - col.label.width / 2.0,
            y: top,
            w: col.label.width,
            h: col.label.height,
        };
        let col = &self.cols[c];
        let skin = self.skin;
        let cx = self.x[c];
        let look = &col.look;
        let stroke = Some(Stroke::new(look.border, look.thickness));
        let text_h = col.stereo_h + col.label.height;
        let left = cx - col.half();
        if let Some((_, icon_h)) = icon(col.shape) {
            let (icon_top, text_top) = if bottom {
                (top + text_h + 3.0, top)
            } else {
                (top, top + icon_h + 3.0)
            };
            draw_icon(canvas, col.shape, cx, icon_top, look);
            let (stereo_top, label_top) = self.stacked(col, text_top);
            stereotype(canvas, col, cx, stereo_top);
            canvas.text(Layer::Front, &col.label, (cx, label_top), Align::Center);
            return (named(cx, label_top, col), None);
        }
        let rx = skin.round;
        let (w, h) = (col.width, col.height);
        let text_top = match col.shape {
            Shape::Collections => {
                canvas.rect(
                    Layer::Front,
                    (left + 4.0, top, w - 4.0, h - 4.0),
                    rx,
                    look.fill,
                    stroke,
                );
                canvas.rect(
                    Layer::Front,
                    (left, top + 4.0, w - 4.0, h - 4.0),
                    rx,
                    look.fill,
                    stroke,
                );
                top + 4.0 + HEAD_PAD
            }
            Shape::Queue => {
                let r = 5.0;
                let (l, rr, t, b) = (left, left + w, top, top + h);
                let d = format!(
                    "M{},{} L{},{} A{},{} 0 0 1 {},{} L{},{} A{},{} 0 0 1 {},{} Z",
                    n(l + r),
                    n(t),
                    n(rr - r),
                    n(t),
                    n(r),
                    n(h / 2.0),
                    n(rr - r),
                    n(b),
                    n(l + r),
                    n(b),
                    n(r),
                    n(h / 2.0),
                    n(l + r),
                    n(t)
                );
                canvas.path(Layer::Front, &d, (l, t, rr, b), look.fill, stroke);
                let cap = format!(
                    "M{},{} A{},{} 0 0 0 {},{}",
                    n(rr - r),
                    n(t),
                    n(r),
                    n(h / 2.0),
                    n(rr - r),
                    n(b)
                );
                canvas.path(Layer::Front, &cap, (rr - 2.0 * r, t, rr, b), None, stroke);
                top + HEAD_PAD
            }
            _ => {
                canvas.rect(Layer::Front, (left, top, w, h), rx, look.fill, stroke);
                top + HEAD_PAD
            }
        };
        let middle = if col.shape == Shape::Collections || col.shape == Shape::Queue {
            cx - 2.0
        } else {
            cx
        };
        let (stereo_top, label_top) = self.stacked(col, text_top);
        stereotype(canvas, col, middle, stereo_top);
        canvas.text_across(
            Layer::Front,
            &col.label,
            (middle, label_top),
            Align::Center,
            (left, left + w),
        );
        (named(middle, label_top, col), look.fill)
    }

    /// Where a head's stereotype and its name stand, from `top` down: the
    /// stereotype over the name, or under it with `stereotypePosition
    /// bottom`.
    fn stacked(&self, col: &Column, top: f32) -> (f32, f32) {
        if self.skin.stereotype_below {
            (top + col.label.height, top)
        } else {
            (top, top + col.stereo_h)
        }
    }
}

/// A column's spot and stereotype, side by side and centered on `cx`, with
/// their top at `top`.
fn stereotype(canvas: &mut Canvas, col: &Column, cx: f32, top: f32) {
    let Some(spot) = &col.spot else {
        canvas.text(Layer::Front, &col.stereo, (cx, top), Align::Center);
        return;
    };
    let left = cx - col.stereo_w / 2.0;
    let r = spot.size / 2.0;
    let middle = top + col.stereo_h / 2.0;
    canvas.ellipse(
        Layer::Front,
        (left + r, middle),
        (r, r),
        spot.fill,
        Some(Stroke::new(col.look.border, 1.0)),
    );
    canvas.text(
        Layer::Front,
        &spot.letter,
        (left + r, middle - spot.letter.height / 2.0),
        Align::Center,
    );
    canvas.text(
        Layer::Front,
        &col.stereo,
        (
            left + spot.size + SPOT_GAP,
            middle - col.stereo.height / 2.0,
        ),
        Align::Left,
    );
}

/// Room for something `width` wide over columns `lo..=hi`.
fn over(space: &mut Space, lo: usize, hi: usize, width: f32) {
    if lo == hi {
        space.before(lo, width / 2.0 + 5.0);
        space.after(lo, width / 2.0 + 5.0);
    } else {
        space.need(lo, hi, width - 2.0 * OVER);
        space.before(lo, OVER + 5.0);
        space.after(hi, OVER + 5.0);
    }
}

/// Left edge and width of something `width` wide over columns `lo..=hi`.
fn over_span(x: &[f32], lo: usize, hi: usize, width: f32) -> (f32, f32) {
    if lo == hi {
        (x[lo] - width / 2.0, width)
    } else {
        let span = x[hi] - x[lo] + 2.0 * OVER;
        let w = span.max(width);
        ((x[lo] + x[hi]) / 2.0 - w / 2.0, w)
    }
}

/// The tab with a cut corner that names a frame.
fn tab(x: f32, y: f32, w: f32, h: f32) -> [(f32, f32); 5] {
    [
        (x, y),
        (x + w, y),
        (x + w, y + h - 6.0),
        (x + w - 6.0, y + h),
        (x, y + h),
    ]
}

fn cross(canvas: &mut Canvas, (x, y): (f32, f32), stroke: Stroke) {
    let s = Stroke {
        dash: None,
        ..stroke
    };
    canvas.line(Layer::Front, (x - 4.0, y - 4.0), (x + 4.0, y + 4.0), s);
    canvas.line(Layer::Front, (x - 4.0, y + 4.0), (x + 4.0, y - 4.0), s);
}

fn keyword(kind: GroupKind, line: usize) -> Text {
    Text {
        value: kind.keyword().into(),
        line,
        col: None,
    }
}

fn note_size(shape: NoteShape, text: &Block) -> (f32, f32) {
    let h = text.height + 10.0;
    match shape {
        NoteShape::Note => (text.width + 22.0, h),
        NoteShape::Hexagon => (text.width + 26.0, h),
        NoteShape::Rectangle => (text.width + 12.0, h),
    }
}

fn draw_note(
    canvas: &mut Canvas,
    skin: &Skin,
    shape: NoteShape,
    (x, y, w, h): (f32, f32, f32, f32),
    text: &Block,
    fill: Paint,
) {
    let look = &skin.note;
    let stroke = Some(Stroke::new(look.border, look.thickness));
    let text_x = match shape {
        NoteShape::Note => {
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
            x + 6.0
        }
        NoteShape::Hexagon => {
            let inset = 8.0;
            canvas.polygon(
                Layer::Front,
                &[
                    (x, y + h / 2.0),
                    (x + inset, y),
                    (x + w - inset, y),
                    (x + w, y + h / 2.0),
                    (x + w - inset, y + h),
                    (x + inset, y + h),
                ],
                fill,
                stroke,
            );
            x + 13.0
        }
        NoteShape::Rectangle => {
            canvas.rect(Layer::Front, (x, y, w, h), 0.0, fill, stroke);
            x + 6.0
        }
    };
    canvas.text_across(
        Layer::Front,
        text,
        (text_x, y + 5.0),
        Align::Left,
        (x + 1.0, x + w - 1.0),
    );
}

/// The icons of actor, boundary, control, entity and database, with their
/// top at `top` and their middle at `cx`.
fn draw_icon(canvas: &mut Canvas, shape: Shape, cx: f32, top: f32, look: &Look) {
    let stroke = Stroke::new(look.border, look.thickness.max(0.5));
    let fill = look.fill;
    let t = top;
    match shape {
        Shape::Actor => {
            canvas.ellipse(Layer::Front, (cx, t + 8.5), (8.0, 8.0), fill, Some(stroke));
            let d = format!(
                "M{x},{a} L{x},{b} M{l},{c} L{r},{c} M{x},{b} L{l},{e} M{x},{b} L{r},{e}",
                x = n(cx),
                a = n(t + 16.5),
                b = n(t + 35.5),
                c = n(t + 22.5),
                e = n(t + 49.5),
                l = n(cx - 13.0),
                r = n(cx + 13.0),
            );
            canvas.path(
                Layer::Front,
                &d,
                (cx - 13.0, t, cx + 13.0, t + 50.0),
                None,
                Some(stroke),
            );
        }
        Shape::Boundary => {
            let l = cx - 17.0;
            canvas.line(Layer::Front, (l, t + 1.0), (l, t + 23.0), stroke);
            canvas.line(Layer::Front, (l, t + 12.0), (cx - 6.0, t + 12.0), stroke);
            canvas.ellipse(
                Layer::Front,
                (cx + 5.0, t + 12.0),
                (11.0, 11.0),
                fill,
                Some(stroke),
            );
        }
        Shape::Control => {
            canvas.ellipse(
                Layer::Front,
                (cx, t + 14.0),
                (11.0, 11.0),
                fill,
                Some(stroke),
            );
            canvas.polyline(
                Layer::Front,
                &[(cx + 4.0, t), (cx - 2.0, t + 3.0), (cx + 4.0, t + 6.0)],
                stroke,
            );
        }
        Shape::Entity => {
            canvas.ellipse(
                Layer::Front,
                (cx, t + 12.0),
                (11.0, 11.0),
                fill,
                Some(stroke),
            );
            canvas.line(
                Layer::Front,
                (cx - 11.0, t + 26.0),
                (cx + 11.0, t + 26.0),
                stroke,
            );
        }
        Shape::Database => {
            let (l, r) = (cx - 15.0, cx + 15.0);
            let d = format!(
                "M{l},{a} L{l},{b} A15,5 0 0 0 {r},{b} L{r},{a}",
                l = n(l),
                r = n(r),
                a = n(t + 5.0),
                b = n(t + 33.0),
            );
            canvas.path(Layer::Front, &d, (l, t, r, t + 38.0), fill, Some(stroke));
            canvas.ellipse(Layer::Front, (cx, t + 5.0), (15.0, 5.0), fill, Some(stroke));
        }
        _ => {}
    }
}

/// The participants an item refers to.
fn participants_of(item: &Item) -> Vec<usize> {
    let ends = |m: &Message| {
        [m.from, m.to]
            .into_iter()
            .filter_map(|e| match e {
                End::Participant(p) => Some(p),
                _ => None,
            })
            .collect()
    };
    match item {
        Item::Message(m) => ends(m),
        Item::Note(note) => match note.place {
            NotePlace::LeftOf(p) | NotePlace::RightOf(p) => vec![p],
            NotePlace::Over(a, b) => vec![a, b],
            _ => Vec::new(),
        },
        Item::Ref { over, .. } => over.clone(),
        Item::Activate { participant, .. }
        | Item::Deactivate { participant, .. }
        | Item::Destroy { participant, .. } => vec![*participant],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A note of its own is a step of a play between the messages, its box
    /// lit with it; a note beside a message is lit with the message
    /// (REQ-106).
    #[test]
    fn notes_are_steps_and_lit_with_their_message() {
        let source = "@startuml\nA -> B : one\nnote right: beside\nnote over A, B: between\nB -> A : two\n@enduml\n";
        let Ok(crate::Diagram::Sequence(d)) = crate::parse(source) else {
            panic!("a sequence");
        };
        let (_, map) = draw_mapped(&d).unwrap();
        assert_eq!(map.messages.len(), 2);
        assert_eq!(map.messages[0].also.len(), 1, "the note beside it");
        assert!(map.messages[1].also.is_empty());
        assert_eq!(map.notes.len(), 1);
        let note = &map.notes[0];
        assert!(note.points.is_empty() && note.also.len() == 1);
        assert_eq!(note.line, 3);
        let [_, y, _, h] = note.also[0];
        assert!(note.top <= y && y + h <= note.bottom);
        // In order down the page: one, the note, two.
        let lines: Vec<usize> = map.steps().iter().map(|s| s.line).collect();
        assert_eq!(lines, [1, 3, 4]);
        // Messages side by side keep the order written, the first of them
        // lower where the second's words are taller; a note beside what
        // stands before it (`/`) reaches as far down, so the veil does not
        // climb back over the message.
        let source = "@startuml\nA -> B : x\n& C -> D : two\\nlines\nnote over A : n\nB -> A : back\n/ note over C : beside\n@enduml\n";
        let Ok(crate::Diagram::Sequence(d)) = crate::parse(source) else {
            panic!("a sequence");
        };
        let (_, map) = draw_mapped(&d).unwrap();
        let steps = map.steps();
        let lines: Vec<usize> = steps.iter().map(|s| s.line).collect();
        assert_eq!(lines, [1, 2, 3, 4, 5]);
        assert!(
            steps[4].bottom >= steps[3].bottom,
            "{:?}",
            (steps[3].bottom, steps[4].bottom)
        );
    }

    /// A box with a fill is a ground of the map, where it is drawn and in
    /// its color, the messages between its participants on it; a box
    /// drawn without a fill is none.
    #[test]
    fn a_filled_box_is_a_ground_of_the_map() {
        let source = "@startuml\nparticipant Alice\nbox \"Shop\" #E0E8FF\nparticipant Bob\nparticipant Carol\nend box\nAlice -> Bob : order\nBob -> Carol : total\n@enduml\n";
        let (_, map) = crate::render_mapped(source, None).unwrap();
        let [ground] = map.grounds[..] else {
            panic!("one ground: {:?}", map.grounds);
        };
        assert_eq!(ground.fill, crate::color::Color::hex(0xE0E8FF));
        let r = ground.rect;
        let on = |(x, y): (f32, f32)| x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h;
        assert!(map.messages[1].points.iter().all(|&p| on(p)), "{r:?}");
        assert!(map.messages[0].points[0].0 < r.x, "Alice is outside");
        // The participants at the top, each on its own with where its
        // name stands: what a pane holds of the band across the top.
        let band = map.bands[0].rect;
        assert_eq!(map.heads.len(), 3, "{:?}", map.heads);
        for head in &map.heads {
            let (r, l) = (head.rect, head.label);
            assert!(
                r.x >= band.x
                    && r.x + r.w <= band.x + band.w
                    && r.y >= band.y
                    && r.y + r.h <= band.y + band.h + 0.01,
                "{r:?} in {band:?}"
            );
            assert!(
                l.x >= r.x && l.x + l.w <= r.x + r.w && l.y >= r.y && l.y + l.h <= r.y + r.h,
                "{l:?} in {r:?}"
            );
            assert!(head.fill.is_some() && head.line > 0.0);
        }
        // What one line alone draws is tied to it; the participants'
        // names are not.
        let lines: Vec<usize> = map.tied.iter().map(|tied| tied.line).collect();
        assert_eq!(lines, [6, 7], "{:?}", map.tied);
        let divided =
            "@startuml\nparticipant Pay\nA -> Pay : go\n== Pay ==\nnote over A : Pay\n@enduml\n";
        let (_, map) = crate::render_mapped(divided, None).unwrap();
        let mut lines: Vec<usize> = map.tied.iter().map(|tied| tied.line).collect();
        lines.sort_unstable();
        assert_eq!(lines, [2, 3, 4]);
        // A bar on a lifeline is the shape of the line that opens it,
        // an `activate` or a message with `++`.
        let bars = "@startuml\nA -> B : go\nactivate B\nB -> C ++ : on\ndeactivate C\ndeactivate B\n@enduml\n";
        let (_, map) = crate::render_mapped(bars, None).unwrap();
        let mut lines: Vec<usize> = map.shapes.iter().map(|shape| shape.line).collect();
        lines.sort_unstable();
        assert_eq!(lines, [2, 3]);
        // A Mermaid sequence diagram names them alike.
        let d = super::super::mermaid::from_mermaid(
            "sequenceDiagram\n  actor A\n  participant B\n  A->>B: hi\n",
        )
        .unwrap();
        let (_, mermaid) = draw_mapped(&d).unwrap();
        assert_eq!(mermaid.heads.len(), 2);
        assert!(
            mermaid.heads[0].fill.is_none(),
            "an actor's name is on the paper"
        );
        let clear = source.replace("#E0E8FF", "#transparent");
        let (_, map) = crate::render_mapped(&clear, None).unwrap();
        assert!(map.grounds.is_empty());
    }

    /// Runs `source` through the layout; `check` sees the finished drawer.
    fn laid_out(source: &str, check: impl FnOnce(&Drawer)) {
        let crate::Diagram::Sequence(d) = crate::parse(source).unwrap() else {
            panic!("not a sequence diagram");
        };
        let skin = Skin::from_params(&d.frame.skinparams).unwrap();
        let (drawer, _) = laid(&d, &skin).unwrap();
        check(&drawer);
    }

    fn columns(source: &str) -> Vec<f32> {
        let mut x = Vec::new();
        laid_out(source, |d| x = d.x.clone());
        x
    }

    fn width(text: &str, size: f32) -> f32 {
        text::block(text, Font::new(size, Color::BLACK), Family::Sans)
            .unwrap()
            .width
    }

    fn count(haystack: &str, needle: &str) -> usize {
        haystack.matches(needle).count()
    }

    #[test]
    fn columns_make_room_for_heads_and_messages() {
        let short = columns("A -> B : hi");
        let label = "a message much longer than both heads";
        let long = columns(&format!("A -> B : {label}"));
        assert!(short[0] < short[1]);
        assert!(long[1] - long[0] >= width(label, 13.0) + 2.0 * TEXT_INSET);
        assert!(long[1] - long[0] > short[1] - short[0]);

        // A message over a middle column spreads the outer ones only.
        let x = columns(&format!("A -> B\nB -> C\nA -> C : {label}"));
        assert!(x[2] - x[0] >= width(label, 13.0) + 2.0 * TEXT_INSET);
    }

    #[test]
    fn self_messages_notes_and_edges_make_room() {
        let text = "talking to itself for a while";
        let x = columns(&format!("A -> A : {text}\nA -> B"));
        assert!(x[1] - x[0] >= width(text, 13.0) + TEXT_INSET + BAR);

        let note = "a note on the right side";
        let x = columns(&format!("A -> B\nnote right of A : {note}"));
        assert!(x[1] - x[0] >= width(note, 13.0) + 22.0);

        let x = columns(&format!("A -> B\nnote left of B : {note}"));
        assert!(x[1] - x[0] >= width(note, 13.0) + 22.0);

        let incoming = "a message from outside";
        let x = columns(&format!("[-> A : {incoming}"));
        assert!(x[0] >= width(incoming, 13.0) + TEXT_INSET);
    }

    #[test]
    fn a_frame_keeps_its_margin_from_the_bars_at_its_sides() {
        // `(x, width)` of the frames (drawn without a fill) and of the bars.
        let rects = |svg: &str, frame: bool| -> Vec<(f32, f32)> {
            svg.split("<rect ")
                .skip(1)
                .filter(|rect| rect.contains("fill=\"none\"") == frame)
                .filter(|rect| frame || rect.contains(" width=\"10\""))
                .map(|rect| {
                    let number = |name: &str| -> f32 {
                        format!(" {rect}")
                            .split(&format!(" {name}=\""))
                            .nth(1)
                            .and_then(|rest| rest.split('"').next()?.parse().ok())
                            .unwrap()
                    };
                    (number("x"), number("width"))
                })
                .collect()
        };
        // The bar on B stands at the frame's left side, then at its right.
        for (body, left) in [
            (
                "A -> B ++ : x\nopt y\nB -> C : z\nend\nB --> A -- : w",
                true,
            ),
            (
                "A -> B ++ : x\nopt y\nA -> B : z\nend\nB --> A -- : w",
                false,
            ),
        ] {
            let svg = crate::render(body).unwrap();
            let (frame, bar) = (rects(&svg, true)[0], rects(&svg, false)[0]);
            let margin = if left {
                bar.0 - frame.0
            } else {
                frame.0 + frame.1 - (bar.0 + bar.1)
            };
            assert!(margin >= FRAME - 0.01, "{body}: {margin}");
        }
        // A bar opened on the sender right after its message.
        let svg = crate::render("opt x\nA -> B : m\nactivate A\nend\nB -> A : back\ndeactivate A")
            .unwrap();
        let (frame, bar) = (rects(&svg, true)[0], rects(&svg, false)[0]);
        assert!(bar.0 - frame.0 >= FRAME - 0.01, "{svg}");
        // Frames nested about B keep clear of the bar on A beside.
        let svg = crate::render(
            "activate A\nA -> B ++ : x\nloop l\nalt a\nopt o\ngroup g\nB -> C : z\nend\nend\nend\nend",
        )
        .unwrap();
        let outer = rects(&svg, true)
            .into_iter()
            .map(|(x, _)| x)
            .fold(f32::MAX, f32::min);
        let on_a = rects(&svg, false)
            .into_iter()
            .map(|(x, w)| x + w)
            .fold(f32::MAX, f32::min);
        assert!(outer - on_a >= FRAME - 0.01, "{outer} {on_a}");
        // Nested at the first lifeline, the frames stay in the picture.
        let svg = crate::render("opt a\nopt b\nA -> B : x\nend\nend").unwrap();
        assert!(rects(&svg, true).iter().all(|(x, _)| *x >= 0.0), "{svg}");
    }

    #[test]
    fn a_frame_s_words_keep_off_the_lifelines_it_ends_by() {
        let number = |tag: &str, name: &str| -> f32 {
            format!(" {tag}")
                .split(&format!(" {name}=\""))
                .nth(1)
                .and_then(|rest| rest.split('"').next()?.parse().ok())
                .unwrap()
        };
        // Where the words in `[…]` end, the frame's right side, and the
        // lifelines by the middles of the heads.
        let measured = |body: &str| {
            let svg = crate::render(body).unwrap();
            let words = svg
                .split("<tspan ")
                .skip(1)
                .find(|tspan| tspan.contains(">[every"))
                .map(|tspan| number(tspan, "x") + number(tspan, "textLength"))
                .unwrap();
            let frame = svg
                .split("<rect ")
                .skip(1)
                .find(|rect| rect.contains("fill=\"none\""))
                .map(|rect| number(rect, "x") + number(rect, "width"))
                .unwrap();
            let mut lifelines: Vec<f32> = svg
                .split("<rect ")
                .skip(1)
                .filter(|rect| rect.contains("rx=\"2.5\""))
                .map(|rect| number(rect, "x") + number(rect, "width") / 2.0)
                .collect();
            lifelines.sort_by(f32::total_cmp);
            lifelines.dedup_by(|a, b| (*a - *b).abs() < 0.01);
            (words, frame, lifelines)
        };
        // Over B and C: the words end their margin before C's lifeline.
        let (words, frame, lifelines) = measured(
            "A -> B : x\nloop every day until the order is shipped\nB -> C : still reserved?\nend",
        );
        assert!(
            words + FRAME <= lifelines[2] + 0.01,
            "{words} {lifelines:?}"
        );
        assert!(words + FRAME <= frame + 0.01, "{words} {frame}");
        // Over B alone: inside the frame, which keeps clear of C.
        let (words, frame, lifelines) = measured(
            "A -> B : x\nloop every day until the order is shipped\nB -> B : again\nend\nB -> C : y",
        );
        assert!(words + FRAME <= frame + 0.01, "{words} {frame}");
        assert!(
            frame + BAR / 2.0 + FRAME <= lifelines[2] + 0.01,
            "{frame} {lifelines:?}"
        );
    }

    #[test]
    fn a_rect_is_as_wide_as_what_it_holds() {
        let drawn = |body: &str, check: &dyn Fn(&Drawer)| {
            let d = crate::sequence::from_mermaid(&format!(
                "sequenceDiagram\n  participant A\n  participant B\n  participant C\n{body}"
            ))
            .unwrap();
            let skin = Skin::from_params(&d.frame.skinparams).unwrap();
            let (drawer, _) = laid(&d, &skin).unwrap();
            check(&drawer);
        };
        // Over B and C, with its room around them: not over A.
        drawn("  rect rgb(240, 248, 255)\n    B->>C: x\n  end\n", &|d| {
            let band = &d.highlights[0];
            assert!(
                band.left > d.x[0] && band.left < d.x[1],
                "{:?} {:?}",
                d.x,
                band.left
            );
            assert!(band.right > d.x[2], "{:?} {:?}", d.x, band.right);
        });
        // A rect inside widens the one around it; one holding nothing
        // spans the participants.
        drawn(
            "  rect rgb(240, 248, 255)\n    rect rgb(200, 200, 255)\n      B->>C: x\n    end\n  end\n  rect rgb(240, 248, 255)\n  end\n",
            &|d| {
                let (inner, outer, empty) = (&d.highlights[0], &d.highlights[1], &d.highlights[2]);
                assert!(outer.left < inner.left && outer.right > inner.right);
                assert!(empty.left < d.x[0] && empty.right > d.x[2]);
            },
        );
    }

    #[test]
    fn a_delay_s_words_stand_between_two_lifelines_where_they_fit() {
        let svg = crate::render(
            "participant Reader\nparticipant Shop\nparticipant Orders\nparticipant Stock\nReader -> Shop : x\n... two days later ...\nShop -> Stock : y",
        )
        .unwrap();
        let number = |tag: &str, name: &str| -> f32 {
            format!(" {tag}")
                .split(&format!(" {name}=\""))
                .nth(1)
                .and_then(|rest| rest.split('"').next()?.parse().ok())
                .unwrap()
        };
        let words = svg
            .split("<tspan ")
            .skip(1)
            .find(|tspan| tspan.contains(">two days later"))
            .map(|tspan| {
                (
                    number(tspan, "x"),
                    number(tspan, "x") + number(tspan, "textLength"),
                )
            })
            .unwrap();
        for lifeline in svg
            .split("<rect ")
            .skip(1)
            .filter(|rect| rect.contains("rx=\"2.5\""))
            .map(|rect| number(rect, "x") + number(rect, "width") / 2.0)
        {
            assert!(
                lifeline <= words.0 - DELAY_ROOM + 0.01 || lifeline >= words.1 + DELAY_ROOM - 0.01,
                "{lifeline} {words:?}"
            );
        }
    }

    #[test]
    fn the_dotted_lifeline_stops_short_of_a_delay_s_words_over_it() {
        laid_out(
            "A -> A : x\n... a long while passes here, longer than the room ...\nA -> A : y",
            |d| {
                let (_, _, words) = d.delays[0];
                assert!(words.is_some_and(|[left, _, right, _]| left < d.x[0] && right > d.x[0]));
            },
        );
        let svg = crate::render(
            "A -> A : x\n... a long while passes here, longer than the room ...\nA -> A : y",
        )
        .unwrap();
        // Two dotted runs, one above the words and one below.
        assert_eq!(svg.matches("stroke-dasharray=\"1,4\"").count(), 2, "{svg}");
        // A delay without words leaves the lone lifeline whole, and asks
        // for no room.
        laid_out("A -> A : x\n...\nA -> A : y", |d| {
            assert!(d.delays[0].2.is_none() && d.drawn.delays.is_empty());
        });
        let svg = crate::render("A -> A : x\n...\nA -> A : y").unwrap();
        assert_eq!(svg.matches("stroke-dasharray=\"1,4\"").count(), 1, "{svg}");
    }

    #[test]
    fn a_rect_that_holds_nothing_widens_no_frame_around_it() {
        let d = crate::sequence::from_mermaid(
            "sequenceDiagram\n  participant A\n  participant B\n  participant C\n  loop every minute\n    B->>C: ping\n    rect rgb(200, 200, 255)\n    end\n  end\n",
        )
        .unwrap();
        let skin = Skin::from_params(&d.frame.skinparams).unwrap();
        let (drawer, _) = laid(&d, &skin).unwrap();
        let frame = &drawer.drawn.frames[0];
        // Around B and C, clear of A's lifeline, which keeps its place.
        assert!(
            frame.left > drawer.x[0] + FRAME - 0.01,
            "{} {:?}",
            frame.left,
            drawer.x
        );
        let without = crate::sequence::from_mermaid(
            "sequenceDiagram\n  participant A\n  participant B\n  participant C\n  loop every minute\n    B->>C: ping\n  end\n",
        )
        .unwrap();
        let (plain, _) = laid(&without, &skin).unwrap();
        assert_eq!(drawer.x, plain.x);
    }

    #[test]
    fn a_number_s_circle_stays_in_its_band_and_off_the_words() {
        let body: String = (1..=10).map(|i| format!("  A->>A: think {i}\n")).collect();
        let d = crate::sequence::from_mermaid(&format!(
            "sequenceDiagram\n  autonumber\n  participant A\n  participant B\n  rect rgb(240, 248, 255)\n    A->>B: hello\n  end\n{body}"
        ))
        .unwrap();
        let skin = Skin::from_params(&d.frame.skinparams).unwrap();
        let (drawer, canvas) = laid(&d, &skin).unwrap();
        let svg = canvas.finish(MARGIN, skin.background);
        // The rect holds the circle at the start of its arrow.
        let band = &drawer.highlights[0];
        let step = &drawer.steps[0];
        let (x, y) = step.points[0];
        let Some(Measured::Message { number, .. }) = drawer
            .measured
            .iter()
            .find(|m| matches!(m, Measured::Message { .. }))
        else {
            panic!("no message");
        };
        let radius = circle_radius(number);
        assert!(band.bottom >= y + radius, "{} {y} {radius}", band.bottom);
        assert!(band.left <= x - radius, "{} {x} {radius}", band.left);
        // A self message's words start clear of the circle of `10`.
        let tenth = &drawer.steps[10];
        let (x, _) = tenth.points[0];
        let words = svg
            .split("<tspan ")
            .find(|tspan| tspan.contains(">think 10<"))
            .and_then(|tspan| {
                tspan
                    .split("x=\"")
                    .nth(1)?
                    .split('"')
                    .next()?
                    .parse::<f32>()
                    .ok()
            })
            .unwrap();
        let ten = drawer
            .measured
            .iter()
            .rev()
            .find_map(|m| match m {
                Measured::Message { number, .. } => Some(circle_radius(number)),
                _ => None,
            })
            .unwrap();
        assert!(words >= x + ten, "{words} {x} {ten}");
    }

    #[test]
    fn a_frame_holds_the_head_of_one_it_creates() {
        let svg = crate::render(
            "participant \"A rather long participant name\" as B\nopt make\ncreate B\nA -> B : new\nend",
        )
        .unwrap();
        let frame = svg
            .split("<rect ")
            .skip(1)
            .find(|rect| rect.contains("fill=\"none\""))
            .unwrap();
        let head = svg
            .split("<rect ")
            .skip(1)
            .find(|rect| rect.contains(" width=\"214."))
            .unwrap_or_else(|| panic!("{svg}"));
        let number = |rect: &str, name: &str| -> f32 {
            format!(" {rect}")
                .split(&format!(" {name}=\""))
                .nth(1)
                .and_then(|rest| rest.split('"').next()?.parse().ok())
                .unwrap()
        };
        assert!(number(frame, "x") >= 0.0, "{svg}");
        assert!(
            number(frame, "x") <= number(head, "x") - FRAME + 0.01,
            "{svg}"
        );
        assert!(
            number(frame, "x") + number(frame, "width")
                >= number(head, "x") + number(head, "width") + FRAME - 0.01,
            "{svg}"
        );
    }

    /// Each case the reviews of F6 and F7 found: a frame's side, its words
    /// or a note beside a lifeline crowding a lifeline or a bar it does not
    /// belong to, a frame past the picture. Drawn with the room found, the
    /// drawing finds no more lacking.
    #[test]
    fn the_room_found_by_drawing_leaves_nothing_crowded() {
        let cases = [
            "participant A\nparticipant B\nparticipant C\nopt y\nA -> B : x\nnote left : n\nactivate B\nend\nB -> C : z",
            "participant \"A rather long participant name\" as C\nparticipant A\nparticipant B\nactivate B\nopt make\ncreate C\nA -> C : new\nend",
            "participant A\nparticipant B\nparticipant \"A rather long participant name\" as C\nactivate A\nopt make\ncreate C\nB -> C : new\nend",
            "participant A\nparticipant B\nA -> B : m\nloop every day until the order is shipped\nB ->] : n\nend\nA ->] : out",
            "A -> C ++ : x\nloop every day until the order is shipped\nB -> C : still\nend\nC --> A --",
            "participant A\nparticipant B\nparticipant C\nB -> C ++ : x\nloop every day until the order is shipped\nB -> C : still\nend\nC --> B --",
            "participant A\nparticipant B\nopt a\nopt b\nnote left of A : hello there\nend\nend",
            "activate A\nactivate A\nactivate A\nA -> B ++ : x\nloop l\nalt a\nopt o\ngroup g\nB -> C : z\nend\nend\nend\nend",
            "A -> B ++ : x\nB -> B ++ : s\nB -> B ++ : t\nloop l\nalt a\nopt o\ngroup g\nA -> B : z\nend\nend\nend\nend\nB -> C ++: w\nC -> D : v",
            "participant A\nparticipant B\nparticipant C\nactivate C\nA -> B ++ : x\nB -> B ++ : s\nB -> B ++ : t\nloop l\nalt a\nopt o\ngroup g\nA -> B : z\nend\nend\nend\nend\nB -> C : w",
            "A -> B ++ : x\nB -> B ++ : s\nB -> B ++ : t\nA -> B : msg\nnote right : a note on the right\nB -> C : z",
            "A -> B ++ : x\nB -> B ++ : s\nB -> B ++ : t\nB -> A : msg\nnote left : a note on the left\nB -> C : z",
            "participant A\nparticipant B\nactivate A\nA -> B : x\nopt o\nnote left of B : a longish note here\nB -> B : y\nend",
            "participant B\nparticipant C\nactivate C\nloop every day until the order is shipped\nnote right of B : n\nend\nB -> C : w",
            "participant A\nparticipant B\nparticipant C\nactivate C\nopt o\nA -> B : m\nB -> B : a longer self text\nend\nB -> C : z",
            "A -> B : x\nloop every day until the order is shipped\nB -> C : still reserved?\nend",
            "participant A\nparticipant B\nopt o\nnote left of B : a longish note here\nend",
            "A -> B ++ : x\nB -> B ++ : s\nB -> B ++ : t\nA -> B : msg\nnote right : a long note on the right",
            "participant A\nparticipant B\ngroup g\n[-> A : request\nend",
            "skinparam sequenceMessageAlign right\nparticipant A\nparticipant B\ngroup g\nB ->] : out\nend",
        ];
        for body in cases {
            let crate::Diagram::Sequence(d) = crate::parse(body).unwrap() else {
                panic!("not a sequence diagram");
            };
            let skin = Skin::from_params(&d.frame.skinparams).unwrap();
            let (drawer, _) = laid(&d, &skin).unwrap();
            let more = drawer.crowded();
            assert!(
                more.is_empty(),
                "{body}: {:?} {:?} {:?}",
                more.need,
                more.left,
                more.right
            );
        }
    }

    #[test]
    fn a_bar_opened_after_a_note_beside_its_message_takes_the_arrow() {
        // The arrow ends at the bar's side, not on the lifeline inside it.
        let body = "A -> B : x\nnote left : n\nactivate B\nB -> A : y";
        laid_out(body, |d| {
            let end = d.steps[0].points.last().unwrap().0;
            assert!((end - (d.x[1] - BAR / 2.0)).abs() < 0.01, "{end} {:?}", d.x);
        });
    }

    #[test]
    fn a_message_creating_a_wide_head_keeps_its_words_between_its_ends() {
        let body = "participant \"A rather long participant name\" as C\nparticipant A\ncreate C\nA -> C : a message with some words";
        laid_out(body, |d| {
            let k =
                d.d.items
                    .iter()
                    .position(|item| matches!(item, Item::Message(_)))
                    .unwrap();
            let room = d.x[1] - (d.x[0] + d.cols[0].half());
            assert!(room >= d.message_text_width(k), "{room} {:?}", d.x);
        });
    }

    #[test]
    fn a_frame_holding_a_message_from_an_edge_reaches_the_edge_once_drawn() {
        for body in [
            "participant A\nparticipant B\ngroup g\n[-> A : request\nend",
            "skinparam sequenceMessageAlign right\nparticipant A\nparticipant B\ngroup g\nB ->] : out\nend",
        ] {
            laid_out(body, |d| {
                let frame = &d.drawn.frames[0];
                assert!(frame.left >= -0.01, "{body}: {}", frame.left);
                assert!(
                    frame.right <= d.right_edge + 0.01,
                    "{body}: {}",
                    frame.right
                );
            });
            // As wide as drawn once: the columns did not move for it.
            let svg = crate::render(body).unwrap();
            let once = {
                let crate::Diagram::Sequence(d) = crate::parse(body).unwrap() else {
                    panic!()
                };
                let skin = Skin::from_params(&d.frame.skinparams).unwrap();
                let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
                let family = canvas.family();
                let mut drawer = Drawer::new(&d, &skin, family).unwrap();
                drawer.place();
                drawer.run(&mut canvas);
                drawer.x.clone()
            };
            let mut x = Vec::new();
            laid_out(body, |d| x = d.x.clone());
            assert_eq!(x, once, "{body}: {svg}");
        }
    }

    #[test]
    fn a_note_beside_a_message_stands_clear_of_the_next_lifeline_as_drawn() {
        let svg = crate::render(
            "A -> B ++ : x\nB -> B ++ : s\nB -> B ++ : t\nA -> B : msg\nnote right : a note on the right\nB -> C : z",
        )
        .unwrap();
        let number = |tag: &str, name: &str| -> Option<f32> {
            format!(" {tag}")
                .split(&format!(" {name}=\""))
                .nth(1)?
                .split('"')
                .next()?
                .parse()
                .ok()
        };
        // The note's outline, a path that closes, ends before C's lifeline,
        // the middle of the rightmost head.
        let note = svg
            .split("<path d=\"M")
            .skip(1)
            .filter_map(|rest| rest.split('"').next()?.strip_suffix(" Z"))
            .flat_map(|outline| {
                outline
                    .split([' ', 'L'])
                    .filter_map(|pair| pair.split(',').next()?.parse::<f32>().ok())
                    .collect::<Vec<_>>()
            })
            .fold(f32::MIN, f32::max);
        let c = svg
            .split("<rect ")
            .skip(1)
            .filter(|rect| rect.contains("rx=\"2.5\""))
            .filter_map(|rect| Some(number(rect, "x")? + number(rect, "width")? / 2.0))
            .fold(f32::MIN, f32::max);
        assert!(note <= c - 5.0 + 0.01, "{note} {c}");
    }

    #[test]
    fn a_note_beside_a_message_stands_beside_the_bars_at_its_end() {
        let body = "A -> B ++ : x\nB -> B ++ : s\nB -> B ++ : t\nA -> B : msg\nnote right : a note on the right";
        let mut x = Vec::new();
        let mut note = 0.0;
        laid_out(body, |d| {
            x = d.x.clone();
            note = d.drawn.notes[0].at[0];
        });
        // B's three bars reach 5 + 2 × 5 past its lifeline.
        assert!(
            note >= x[1] + BAR / 2.0 + 2.0 * BAR_SHIFT + 5.0 - 0.01,
            "{note} {x:?}"
        );
    }

    #[test]
    fn heads_repeat_below_unless_hidden_or_destroyed() {
        let svg = crate::render("A -> B\nA -> C").unwrap();
        assert_eq!(count(&svg, ">A</tspan>"), 2);
        let svg = crate::render("hide footbox\nA -> B").unwrap();
        assert_eq!(count(&svg, ">A</tspan>"), 1);
        let svg = crate::render("A -> B\ndestroy B").unwrap();
        assert_eq!(count(&svg, ">B</tspan>"), 1);
        let svg = crate::render("participant X\nA -> B\nhide unlinked").unwrap();
        assert_eq!(count(&svg, ">X</tspan>"), 0);
    }

    /// A note leaves the message before it the last arrow, as PlantUML
    /// reads it: a participant destroyed or a bar opened after the note
    /// ends or starts at that message; after a group it is a row of its
    /// own.
    #[test]
    fn a_note_leaves_its_message_the_cross_and_the_bar() {
        let mut arrow = 0.0;
        laid_out("A -> B : q\ndestroy B", |d| arrow = d.life_end[1].unwrap());
        laid_out(
            "A -> B : q\nnote over B : x\nnote over A, B : y\ndestroy B",
            |d| {
                assert_eq!(d.life_end[1], Some(arrow));
            },
        );
        laid_out("A -> B : q\nnote over B : x\nactivate B\nB -> A : r", |d| {
            assert_eq!(d.bars[0].start, arrow);
        });
        laid_out("A -> B : q\nalt ok\n  A -> B : r\nend\ndestroy B", |d| {
            assert!(d.life_end[1].unwrap() > arrow + 40.0);
        });
        // The message stops at the bar it opens across the note, its
        // edge, as it does at one opened right after it; a message to
        // itself comes back to the edge too.
        let tip = |source: &str| {
            let svg = crate::render(source).unwrap();
            let arrow = svg.split("<polygon points=\"").nth(1).unwrap();
            let first = arrow.split(' ').nth(1).unwrap();
            first.split(',').next().unwrap().parse::<f32>().unwrap()
        };
        assert_eq!(
            tip("A -> B : q\nnote over B : x\nactivate B\nB -> A : r"),
            tip("A -> B : q\nactivate B\nnote over B : x\nB -> A : r")
        );
        let (mut alone, mut noted) = (Vec::new(), Vec::new());
        laid_out("A -> A : self\nactivate A", |d| alone = d.bars.clone());
        laid_out("A -> A : self\nnote left of A : n\nactivate A", |d| {
            noted = d.bars.clone()
        });
        assert_eq!(alone[0].start, noted[0].start);
    }

    /// Mermaid opens and closes a bar after a note below the note: its
    /// notes leave no message the last arrow.
    #[test]
    fn a_mermaid_note_keeps_the_bar_below_it() {
        let d = super::super::from_mermaid(
            "sequenceDiagram\n  A->>B: q\n  Note over B: a note\n  activate B\n  B->>A: r\n  deactivate B\n",
        )
        .unwrap();
        assert!(!d.through_notes);
        let skin = Skin::from_params(&d.frame.skinparams).unwrap();
        let (drawer, _) = laid(&d, &skin).unwrap();
        let mut plantuml = 0.0;
        laid_out(
            "A -> B : q\nnote over B : a note\nactivate B\nB -> A : r",
            |d| plantuml = d.bars[0].start,
        );
        assert!(
            drawer.bars[0].start > plantuml + 20.0,
            "{}",
            drawer.bars[0].start
        );
    }

    #[test]
    fn created_participants_start_at_their_message() {
        laid_out(
            "A -> B : one\ncreate C\nA -> C : new\nC -> C : self\ndestroy C",
            |d| {
                let (a, c) = (d.life_start[0], d.life_start[2]);
                assert!(c > a + 20.0, "{a} {c}");
                let end = d.life_end[2].unwrap();
                assert!(end > c);
            },
        );
        // With a note between `create` and the message, the head still
        // waits for the message; `**` on it is the same creation.
        for source in [
            "A -> B : one\ncreate C\nnote over A : wait\nA -> C : new",
            "A -> B : one\ncreate C\nnote over A : wait\nA -> C ** : new",
        ] {
            laid_out(source, |d| {
                let head = d.life_start[2] - d.cols[2].height / 2.0;
                assert!(d.cols[2].created_by == Some(2), "{source}");
                assert!(head > d.life_start[0] + 40.0, "{source}: {head}");
            });
        }
    }

    #[test]
    fn activations_follow_their_messages() {
        laid_out(
            "A -> B ++ : call\nB -> C ++ : more\nC --> B -- : done\nB --> A -- : done",
            |d| {
                assert_eq!(d.bars.len(), 2);
                let b = d.bars.iter().find(|b| b.column == 1).unwrap();
                let c = d.bars.iter().find(|b| b.column == 2).unwrap();
                assert!(
                    b.start < c.start && c.end < b.end,
                    "{} {} {} {}",
                    b.start,
                    c.start,
                    c.end,
                    b.end
                );
            },
        );
        // Nested on the same participant: shifted to the right.
        laid_out("A -> A ++ : one\nA -> A ++ : two", |d| {
            let depths: Vec<usize> = d.bars.iter().map(|b| b.depth).collect();
            assert_eq!(depths, [0, 1]);
        });
    }

    #[test]
    fn frames_hold_what_is_inside() {
        let svg = crate::render("A -> B\nloop forever\n  B -> C : again\nend").unwrap();
        assert!(svg.contains(">loop</tspan>") && svg.contains(">[forever]</tspan>"));
        laid_out(
            "A -> B\nalt x\n  B -> C\n  opt y\n    B -> C\n  end\nend",
            |d| {
                assert!(d.groups.is_empty());
            },
        );
    }

    /// `&` puts a message at the height of the one before: the arrows
    /// lie on one line, under the taller text, and what follows comes
    /// after both.
    #[test]
    fn messages_together_share_their_arrow_height() {
        let arrows = |source: &str| {
            let mut ys = Vec::new();
            laid_out(source, |d| {
                ys = d.steps.iter().map(|step| step.points[0].1).collect();
            });
            ys
        };
        let ys = arrows("A -> B : one\n& B -> C : two\\nlines\n& C -> D\nA -> B : next");
        assert_eq!(ys[0], ys[1]);
        assert_eq!(ys[0], ys[2]);
        let alone = arrows("B -> C : two\\nlines\nA -> B : next");
        assert_eq!(&ys[2..], &alone[..], "as far down as the tallest alone");

        // A note beside a message begins where the message begins.
        let mut heights = (0.0, 0.0);
        laid_out("A -> B : one\n& note over C : c\\nc\\nc\\nc", |d| {
            heights.0 = d.y
        });
        laid_out("note over C : c\\nc\\nc\\nc", |d| heights.1 = d.y);
        assert_eq!(heights.0, heights.1);
    }

    /// A note that would lie on the note before it stands below it.
    #[test]
    fn a_note_on_the_note_before_stands_below() {
        let height = |body: &str| {
            let mut y = 0.0;
            laid_out(body, |d| y = d.y);
            y
        };
        let below = height("note over A : one\nnote over A : two");
        assert_eq!(height("note over A : one\n/ note over A : two"), below);
        assert_eq!(height("note over A : one\n/ note across : two"), below);
        assert_eq!(height("note over A, B : one\n/ note over B : two"), below);
        // Left and right of one lifeline there is room for both.
        assert!(height("note left of A : one\n/ note right of A : two") < below);
        assert!(height("note over A : one\n/ note over B : two") < below);
    }

    /// Notes at one height stand beside one another, however wide.
    #[test]
    fn notes_together_make_room_for_one_another() {
        let wide = "a note that is wide enough";
        let half = width(wide, 13.0) / 2.0;
        let apart = columns(&format!("note over A : {wide}\n/ note over B : {wide}"));
        assert!(apart[1] - apart[0] >= 2.0 * half + NOTE_GAP, "{apart:?}");
        let stacked = columns(&format!("note over A : {wide}\nnote over B : {wide}"));
        assert!(stacked[1] - stacked[0] < apart[1] - apart[0]);
        let sides = columns(&format!(
            "note right of A : {wide}\n/ note left of B : {wide}"
        ));
        assert!(sides[1] - sides[0] >= 4.0 * half + NOTE_GAP, "{sides:?}");
    }

    #[test]
    fn declines_what_it_cannot_draw() {
        for (source, line, part) in [
            ("participant A #red/blue", Some(0), "gradients"),
            ("title alone", None, "nothing but its title"),
            ("skinparam shadowing false", None, "the diagram is empty"),
            ("", None, "the diagram is empty"),
            ("== Part ==", None, "needs a participant"),
        ] {
            let err = crate::render(source).unwrap_err();
            assert_eq!(err.line, line, "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
            // Nothing at all is a mistake PlantUML would only greet; a title
            // alone PlantUML draws.
            assert!(crate::render("").unwrap_err().refused);
            assert!(!crate::render("title alone").unwrap_err().refused);
        }
    }

    /// Its participants, messages, notes and fragments carry ids from the
    /// source, but the picture is drawn as a whole: a pane fades it and a
    /// build uncovers it from the top down (docs/adr/0037).
    #[test]
    fn its_elements_have_ids_from_the_source() {
        let source = "actor User\nparticipant \"Web app\" as Web\nbox Back\nparticipant Api\nend box\nUser -> Web : log in\nnote right : first\nactivate Web\nalt ok\n  Web -> Api : check\nend\n== Done ==\nnote over User : bye\n...later...\nref over User, Web : more\nUser -> Api\ndestroy Api";
        let svg = crate::render(source).unwrap();
        for id in [
            "actor-User",
            "participant-Web",
            "participant-Api",
            "box-Back",
            "message-User-Web-log_20_in",
            "note-message-User-Web-log_20_in-right-1",
            "alt-ok",
            "message-Web-Api-check",
            "divider-Done",
            "note-actor-User-over-1",
            "delay-later",
            "ref-more",
            "message-User-Api",
        ] {
            assert_eq!(count(&svg, &format!(" id=\"{id}\"")), 1, "{id}");
        }
        // A participant's head, foot, lifeline and bar are all its own;
        // its id is on its head, not on the lifeline under it.
        assert!(count(&svg, r#"<g class="participant-Web""#) >= 3, "{svg}");
        let head = r#"<g class="participant-Web" id="participant-Web">"#;
        let at = svg.find(head).unwrap() + head.len();
        assert!(!svg[at..].starts_with("<line"), "{}", &svg[at..at + 200]);
        assert!(svg.contains(crate::NAMED) && !svg.contains(crate::svg::MARKED));
        assert!(crate::has_parts(&svg) && !crate::glides_by_parts(&svg));
        // The same source, the same ids.
        assert_eq!(svg, crate::render(source).unwrap());
    }

    /// A box with no one to show is not drawn, and the next keeps its own
    /// name.
    #[test]
    fn a_box_keeps_its_name_after_one_not_drawn() {
        let svg = crate::render(
            "hide unlinked\nbox Idle\nparticipant X\nend box\nbox Back\nparticipant Api\nend box\nUser -> Api : go",
        )
        .unwrap();
        assert!(
            svg.contains(r#" id="box-Back""#) && !svg.contains("box-Idle"),
            "{svg}"
        );
    }

    #[test]
    fn monochrome_and_colors() {
        let svg = crate::render("participant A #ff0000\nA -[#00ff00]> A").unwrap();
        assert!(svg.contains("#FF0000") && svg.contains("#00FF00"));
        let svg =
            crate::render("skinparam monochrome true\nparticipant A #ff0000\nA -> A").unwrap();
        assert!(!svg.contains("#FF0000") && svg.contains("#4D4D4D"));
    }

    /// The cross of a destroyed participant is PlantUML's red. The
    /// lifelines' color sets it too, and so does a style for `destroy`:
    /// of the two the later wins, as in PlantUML. Monochrome grays it.
    #[test]
    fn a_destroyed_participant_s_cross_is_red_until_set() {
        let cross = |head: &str| {
            let svg = crate::render(&format!("{head}\nA -> B\ndestroy B")).unwrap();
            let strokes: Vec<&str> = svg
                .split("<line ")
                .skip(1)
                .filter(|l| l.contains(r#"stroke-width="2""#))
                .filter_map(|l| l.split(r#"stroke=""#).nth(1)?.split('"').next())
                .collect();
            assert_eq!(strokes.len(), 2, "{svg}");
            assert_eq!(strokes[0], strokes[1]);
            strokes[0].to_owned()
        };
        assert_eq!(cross(""), "#A80036");
        let lifeline = "skinparam SequenceLifeLineBorderColor #00AA00";
        let style = "<style>\ndestroy { LineColor #0000FF }\n</style>";
        assert_eq!(cross(lifeline), "#00AA00");
        assert_eq!(cross(&format!("{lifeline}\n{style}")), "#0000FF");
        assert_eq!(cross(&format!("{style}\n{lifeline}")), "#00AA00");
        let nested =
            "<style>\nsequenceDiagram {\n  lifeLine {\n    LineColor #00AA00\n  }\n}\n</style>";
        assert_eq!(cross(nested), "#00AA00");
        assert_eq!(cross("skinparam monochrome true"), "#383838");
        // PlantUML takes no skinparam for the cross alone, nor the short
        // name for the lifelines, which leaves it red.
        assert!(crate::render("skinparam SequenceDestroyColor #0000FF\nA -> B").is_err());
        assert_eq!(cross("skinparam LifeLineBorderColor #00AA00"), "#A80036");
        // Only its color is drawn so far.
        assert!(crate::render("<style>\ndestroy { LineThickness 5 }\n</style>\nA -> B").is_err());
    }
}
