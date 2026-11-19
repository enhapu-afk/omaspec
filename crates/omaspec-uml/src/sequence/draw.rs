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
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    canvas.sketch(skin.sketch);
    canvas.shadows(skin.shadow);
    let family = canvas.family();
    let mut drawer = Drawer::new(d, &skin, family)?;
    drawer.place();
    drawer.run(&mut canvas);
    let map = Map {
        messages: std::mem::take(&mut drawer.steps),
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
    },
    Else {
        label: Block,
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
struct BoxPlace {
    first: usize,
    last: usize,
    title: Block,
    look: Look,
}

/// An activation bar.
struct Bar {
    column: usize,
    depth: usize,
    start: f32,
    end: f32,
    fill: Paint,
    /// The line that opened it.
    line: usize,
}

struct OpenGroup {
    top: f32,
    header: Block,
    label: Block,
    /// The line that opens it.
    line: usize,
    /// Each `else`: where, its words and its line.
    elses: Vec<(f32, Block, usize)>,
    /// What it holds, left and right.
    lo: f32,
    hi: f32,
}

struct Drawer<'a> {
    d: &'a Diagram,
    skin: &'a Skin,
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
    delays: Vec<(f32, f32)>,
    groups: Vec<OpenGroup>,
    dividers: Vec<(f32, Block, usize)>,
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
struct OpenRect {
    top: f32,
    fill: Paint,
}

/// One closed `rect`, across the participant band.
struct Highlight {
    top: f32,
    bottom: f32,
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
            let label = measure(
                &Text {
                    value: p.label.clone(),
                    line: p.line,
                    col: None,
                },
                look.font,
                family,
            )?;
            let stereo = match &p.stereotype {
                Some(s) => measure(
                    &Text {
                        value: format!("«{s}»"),
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
                Item::Message(m) => Measured::Message {
                    text: measure_opt(m.text.as_ref(), skin.arrow_font, family)?,
                    number: match &m.number {
                        Some(number) => measure(
                            &Text {
                                value: number.clone(),
                                line: m.line,
                                col: None,
                            },
                            skin.arrow_font,
                            family,
                        )?,
                        None => Block::empty(),
                    },
                    color: solid_at(m.arrow.color.as_ref(), m.line)?,
                },
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
                    Measured::Group { header, label }
                }
                Item::GroupElse { label, .. } => Measured::Else {
                    label: bracketed(label.as_ref(), skin.group_label_font, family)?,
                },
                Item::Divider { label } => {
                    Measured::Label(measure(label, skin.divider.font, family)?)
                }
                Item::Delay { label, .. } => {
                    Measured::Label(measure_opt(label.as_ref(), skin.delay_font, family)?)
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
        for b in &d.boxes {
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
            if first == last {
                let more = ((title.width + 10.0 - cols[first].width) / 2.0).max(skin.box_padding);
                pad_left[first] = pad_left[first].max(more);
                pad_right[first] = pad_right[first].max(more);
            }
            boxes.push(BoxPlace {
                first,
                last,
                title,
                look,
            });
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
        Ok(Drawer {
            d,
            skin,
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
            groups: Vec::new(),
            dividers: Vec::new(),
            last_arrow: None,
            level: None,
            beside,
            steps: Vec::new(),
            grounds: Vec::new(),
            band: None,
            heads: Vec::new(),
            open_rects: Vec::new(),
            highlights: Vec::new(),
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

    /// From where a message begins down to its arrow: its text is above.
    fn arrow_drop(&self, k: usize) -> Option<f32> {
        match &self.measured[k] {
            Measured::Message { text, number, .. } => {
                let height = text.height.max(number.height);
                Some(height + if height > 0.0 { 2.0 } else { 3.0 })
            }
            _ => None,
        }
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
                let inner = self.cols[b.first].half()
                    + self.pad_left[b.first]
                    + self.cols[b.last].half()
                    + self.pad_right[b.last];
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
                (Item::GroupStart { .. }, Measured::Group { header, label }) => {
                    let width = header.width
                        + 18.0
                        + if label.is_empty() {
                            0.0
                        } else {
                            label.width + 5.0
                        };
                    open.push((None, width + 10.0, Vec::new(), Vec::new()));
                }
                (Item::GroupElse { .. }, Measured::Else { label }) => {
                    if let Some(last) = open.last_mut() {
                        last.1 = last.1.max(label.width + 15.0);
                    }
                }
                (Item::GroupEnd { .. }, _) => {
                    if let Some((span, width, inner, heads)) = open.pop() {
                        let (lo, hi) = span.unwrap_or((0, n - 1));
                        if lo < hi {
                            space.need(lo, hi, width - 2.0 * FRAME);
                        } else {
                            space.after(lo, width - FRAME + 5.0);
                        }
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
                    let w = self.message_text_width(k);
                    let room = w + MESSAGE_ROOM;
                    let (from, to) = (self.end_col(m.from), self.end_col(m.to));
                    let (lo, hi, self_loop) = match (from, to) {
                        (Some(a), Some(b)) if a == b => {
                            let reach = (w + 2.0 * TEXT_INSET).max(LOOP_WIDTH + 10.0) + BAR;
                            space.after(a, reach + 5.0);
                            (a, a, Some(reach))
                        }
                        (Some(a), Some(b)) => {
                            space.need(a, b, room + BAR);
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
        let (x, right) = space.solve();
        self.x = x;
        self.right_edge = right;
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

    /// Widens every open frame to hold `lo..hi`.
    fn touch(&mut self, lo: f32, hi: f32) {
        for g in &mut self.groups {
            g.lo = g.lo.min(lo);
            g.hi = g.hi.max(hi);
        }
    }

    fn run(&mut self, canvas: &mut Canvas) {
        let skin = self.skin;
        let mut top = self.annotations.top();
        let content_top = top;
        let boxes_top = top;
        if !self.boxes.is_empty() {
            let title = self
                .boxes
                .iter()
                .map(|b| b.title.height)
                .fold(0.0, f32::max);
            top += title + if title > 0.0 { 8.0 } else { 5.0 };
        }
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
        // The heads stay over the lifelines, not over the feet below them.
        self.band = heads_band.map(|rect| crate::map::Band {
            rect,
            edge: crate::map::Edge::Top,
            over: crate::graph::Rect {
                x: rect.x,
                y: rect.y + rect.h,
                w: rect.w,
                h: (end - rect.y - rect.h).max(0.0),
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
            canvas.rect(
                Layer::Bars,
                rect,
                0.0,
                bar.fill,
                Some(Stroke::new(skin.lifeline, 1.0)),
            );
            canvas.shape_of(bar.line, rect);
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
        for b in &self.boxes {
            let left = self.x[b.first] - self.cols[b.first].half() - self.pad_left[b.first];
            let right = self.x[b.last] + self.cols[b.last].half() + self.pad_right[b.last];
            let rect = (left, boxes_top, right - left, bottom + 5.0 - boxes_top);
            canvas.rect(
                Layer::Background,
                rect,
                skin.round,
                b.look.fill,
                Some(Stroke::new(b.look.border, b.look.thickness)),
            );
            if let Some(fill) = b.look.fill {
                self.grounds.push(crate::map::Ground {
                    rect: crate::graph::Rect {
                        x: rect.0,
                        y: rect.1,
                        w: rect.2,
                        h: rect.3,
                    },
                    // As the canvas paints it: gray in monochrome.
                    fill: if skin.monochrome { fill.gray() } else { fill },
                });
            }
            canvas.text(
                Layer::Background,
                &b.title,
                ((left + right) / 2.0, boxes_top + 4.0),
                Align::Center,
            );
        }
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
        for (cy, label, line) in dividers {
            canvas.tie(Some(line));
            self.divider(canvas, cy, &label, left, right);
        }
        canvas.tie(None);
    }

    /// Draws the item at `k` (and the note beside it); the next item.
    fn item(&mut self, canvas: &mut Canvas, k: usize) -> usize {
        let d = self.d;
        let item = &d.items[k];
        if !matches!(
            item,
            Item::Activate { .. } | Item::Deactivate { .. } | Item::Destroy { .. }
        ) {
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
            _ => None,
        });
        let next = self.item_of(canvas, k);
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
                if let Measured::Group { header, label } = &self.measured[k] {
                    let (header, label) = (header.clone(), label.clone());
                    let top = self.y + 7.0;
                    let tab = header.height + 4.0;
                    self.y = top + tab.max(label.height + 4.0);
                    self.groups.push(OpenGroup {
                        top,
                        header,
                        label,
                        line: *line,
                        elses: Vec::new(),
                        lo: f32::INFINITY,
                        hi: f32::NEG_INFINITY,
                    });
                }
            }
            Item::GroupElse { line, .. } => {
                if let Measured::Else { label } = &self.measured[k] {
                    let label = label.clone();
                    let y = self.y + 7.0;
                    self.y = y + if label.is_empty() {
                        3.0
                    } else {
                        label.height + 2.0
                    };
                    if let Some(g) = self.groups.last_mut() {
                        g.elses.push((y, label, *line));
                    }
                }
            }
            Item::GroupEnd { .. } => self.group_end(canvas),
            Item::Divider { label: written } => {
                if let Measured::Label(label) = &self.measured[k] {
                    let label = label.clone();
                    let h = label.height + 6.0;
                    let cy = self.y + 10.0 + h / 2.0;
                    self.dividers.push((cy, label, written.line));
                    self.y = cy + h / 2.0 + 7.0;
                }
            }
            Item::Delay { .. } => {
                if let Measured::Label(label) = &self.measured[k] {
                    let label = label.clone();
                    let start = self.y + 5.0;
                    let end = start + 20.0 + label.height;
                    let middle = (self.x[0] + self.x[self.x.len() - 1]) / 2.0;
                    canvas.text(Layer::Front, &label, (middle, start + 10.0), Align::Center);
                    self.delays.push((start, end));
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
                self.open_rects.push(OpenRect { top: self.y, fill });
            }
            Item::RectEnd { .. } => {
                if let Some(open) = self.open_rects.pop() {
                    let bottom = self.y;
                    self.highlights.push(Highlight {
                        top: open.top,
                        bottom,
                        fill: open.fill,
                    });
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
                let stroke = Stroke::new(self.skin.lifeline, 2.0);
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
        }
        k + 1
    }

    /// `rect` bands, across every lifeline, with a little padding.
    fn paint_highlights(&self, canvas: &mut Canvas) {
        if self.x.is_empty() {
            return;
        }
        let last = self.x.len() - 1;
        let left = self.x[0] - self.cols[0].half() - 8.0;
        let right = self.x[last] + self.cols[last].half() + 8.0;
        for band in self.highlights.iter().rev() {
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
                .take_while(|i| {
                    matches!(
                        i,
                        Item::Activate { .. } | Item::Deactivate { .. } | Item::Destroy { .. }
                    )
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
        let mut arrow_y = top + text_h + if text_h > 0.0 { 2.0 } else { 3.0 };
        if let Some(c) = created {
            arrow_y = arrow_y.max(self.y + self.cols[c].height / 2.0 + 4.0);
        }
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
        let mut lifecycle_y = arrow_y;
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
            );
            if m.arrow.tail != Head::None {
                self.arrow_end(
                    canvas,
                    (start, arrow_y),
                    -1.0,
                    m.arrow.tail,
                    m.arrow.start_mark,
                    stroke,
                );
            }
            points = vec![
                (start, arrow_y),
                (out, arrow_y),
                (out, bottom),
                (back, bottom),
            ];
            text_left = start + TEXT_INSET;
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
            self.arrow_line(canvas, sx, tx, arrow_y, dir, m, stroke);
            points = vec![(sx, arrow_y), (tx, arrow_y)];
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
            text_left = match skin.message_align {
                MessageAlign::Left => l + TEXT_INSET + head_at(true),
                MessageAlign::Center => (l + r - text_w) / 2.0,
                MessageAlign::Right => r - TEXT_INSET - head_at(false) - text_w,
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
        canvas.text(Layer::Front, &number, (text_left, top), Align::Left);
        let gap = if number.is_empty() {
            0.0
        } else {
            number.width + 4.0
        };
        canvas.text(Layer::Front, &text, (text_left + gap, top), Align::Left);
        self.touch(reach.0, reach.1);

        if let Some(c) = created {
            let head_top = arrow_y - self.cols[c].height / 2.0;
            self.head(canvas, c, head_top, false);
            self.head_drawn[c] = true;
            self.life_start[c] = head_top + self.cols[c].height;
            after = after.max(head_top + self.cols[c].height + 3.0);
        }

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
            let nx = match side {
                Side::Left => lo - 5.0 - width,
                Side::Right => hi + 5.0,
            };
            let ny = self.y + 7.0;
            draw_note(
                canvas,
                skin,
                note.shape,
                (nx, ny, width, height),
                note_text,
                *fill,
            );
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
        });
    }

    /// The line of a message between `sx` and `tx`, with its heads and marks.
    #[allow(clippy::too_many_arguments)]
    fn arrow_line(
        &self,
        canvas: &mut Canvas,
        sx: f32,
        tx: f32,
        y: f32,
        dir: f32,
        m: &Message,
        stroke: Stroke,
    ) {
        let a = &m.arrow;
        let mut start = sx;
        let mut end = tx;
        match a.end_mark {
            Mark::Circle => {
                canvas.ellipse(
                    Layer::Front,
                    (tx - dir * 4.0, y),
                    (4.0, 4.0),
                    None,
                    Some(Stroke {
                        dash: None,
                        ..stroke
                    }),
                );
                end = tx - dir * 8.0;
            }
            Mark::Cross => {}
            Mark::None => {}
        }
        match a.start_mark {
            Mark::Circle => {
                canvas.ellipse(
                    Layer::Front,
                    (sx + dir * 4.0, y),
                    (4.0, 4.0),
                    None,
                    Some(Stroke {
                        dash: None,
                        ..stroke
                    }),
                );
                start = sx + dir * 8.0;
            }
            Mark::Cross => cross(canvas, (sx, y), stroke),
            Mark::None => {}
        }
        canvas.line(Layer::Front, (start, y), (end, y), stroke);
        if a.end_mark == Mark::Cross {
            cross(canvas, (tx, y), stroke);
        } else {
            self.arrow_end(canvas, (end, y), dir, a.head, Mark::None, stroke);
        }
        if a.tail != Head::None {
            self.arrow_end(canvas, (start, y), -dir, a.tail, Mark::None, stroke);
        }
    }

    /// A head with its tip at `tip`, pointing in `dir` (1 right, -1 left).
    fn arrow_end(
        &self,
        canvas: &mut Canvas,
        (x, y): (f32, f32),
        dir: f32,
        head: Head,
        mark: Mark,
        stroke: Stroke,
    ) {
        if mark == Mark::Cross {
            cross(canvas, (x, y), stroke);
            return;
        }
        let solid = Stroke {
            dash: None,
            width: stroke.width.min(1.5),
            ..stroke
        };
        let back = x - dir * HEAD_LENGTH;
        let fill = Some(stroke.color);
        match head {
            Head::None => {}
            Head::Filled if self.skin.strict => canvas.polygon(
                Layer::Front,
                &[(back, y - HEAD_HALF), (x, y), (back, y + HEAD_HALF)],
                fill,
                Some(solid),
            ),
            Head::Filled => canvas.polygon(
                Layer::Front,
                &[
                    (back, y - HEAD_HALF),
                    (x, y),
                    (back, y + HEAD_HALF),
                    (x - dir * 6.0, y),
                ],
                fill,
                Some(solid),
            ),
            Head::Open => canvas.polyline(
                Layer::Front,
                &[(back, y - HEAD_HALF), (x, y), (back, y + HEAD_HALF)],
                solid,
            ),
            Head::UpperFilled => canvas.polygon(
                Layer::Front,
                &[(back, y - HEAD_HALF), (x, y), (back, y)],
                fill,
                Some(solid),
            ),
            Head::LowerFilled => canvas.polygon(
                Layer::Front,
                &[(back, y + HEAD_HALF), (x, y), (back, y)],
                fill,
                Some(solid),
            ),
            Head::UpperOpen => canvas.line(Layer::Front, (back, y - HEAD_HALF), (x, y), solid),
            Head::LowerOpen => canvas.line(Layer::Front, (back, y + HEAD_HALF), (x, y), solid),
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
        let x = match note.place {
            NotePlace::LeftOf(p) => {
                let c = self.col(p);
                self.bar_edge(c, Side::Left, 0) - 5.0 - width
            }
            NotePlace::RightOf(p) => {
                let c = self.col(p);
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
        let y = self.y + 7.0;
        draw_note(
            canvas,
            self.skin,
            note.shape,
            (x, y, width, height),
            &text,
            fill,
        );
        self.touch(x, x + width);
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
        let left = lo - FRAME;
        let mut right = (hi + FRAME).max(left + tab_w + 10.0);
        if !g.label.is_empty() {
            right = right.max(left + tab_w + 5.0 + g.label.width + 10.0);
        }
        for (_, label, _) in &g.elses {
            right = right.max(left + label.width + 15.0);
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
            skin.group_header_fill,
            Some(border),
        );
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
        self.y = bottom;
        self.touch(left, right);
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
        let dashed = Stroke::new(skin.lifeline, skin.lifeline_thickness).dashed(5.0, 5.0);
        let dotted = Stroke::new(skin.lifeline, skin.lifeline_thickness.max(1.0)).dashed(1.0, 4.0);
        for c in 0..self.cols.len() {
            if !self.head_drawn[c] {
                continue;
            }
            let x = self.x[c];
            let mut y = self.life_start[c];
            let stop = self.life_end[c].unwrap_or(end);
            for &(d0, d1) in &self.delays {
                if d1 <= y || d0 >= stop {
                    continue;
                }
                if d0 > y {
                    canvas.line(Layer::Lifelines, (x, y), (x, d0), dashed);
                }
                let to = d1.min(stop);
                canvas.line(Layer::Lifelines, (x, d0.max(y)), (x, to), dotted);
                y = to;
            }
            if stop > y {
                canvas.line(Layer::Lifelines, (x, y), (x, stop), dashed);
            }
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
        // A participant's name is no one line's: many mention it, also
        // where a message's line creates it.
        let line = canvas.tie(None);
        // At the foot it is the head's name again.
        let before = canvas.echo(bottom);
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
            stereotype(canvas, col, cx, text_top);
            canvas.text(
                Layer::Front,
                &col.label,
                (cx, text_top + col.stereo_h),
                Align::Center,
            );
            return (named(cx, text_top + col.stereo_h, col), None);
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
        stereotype(canvas, col, middle, text_top);
        canvas.text(
            Layer::Front,
            &col.label,
            (middle, text_top + col.stereo_h),
            Align::Center,
        );
        (named(middle, text_top + col.stereo_h, col), look.fill)
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
    canvas.text(Layer::Front, text, (text_x, y + 5.0), Align::Left);
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
        let mut canvas = Canvas::new(&skin.font_name, false);
        let mut drawer = Drawer::new(&d, &skin, Family::Sans).unwrap();
        drawer.place();
        drawer.run(&mut canvas);
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

    #[test]
    fn a_note_beside_a_message_stands_by_its_lifeline_not_by_the_bars() {
        let svg = crate::render(
            "A -> B ++ : x\nB -> B ++ : s\nB -> B ++ : t\nA -> B : msg\nnote right : a note on the right\nB -> C : z",
        )
        .unwrap();
        // The note's outline ends before C's lifeline.
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
        // C's lifeline, by the middle of its head, the rightmost.
        let c = svg
            .split("<rect ")
            .skip(1)
            .filter(|rect| rect.contains("rx=\"2.5\""))
            .filter_map(|rect| {
                let number = |name: &str| -> Option<f32> {
                    format!(" {rect}")
                        .split(&format!(" {name}=\""))
                        .nth(1)?
                        .split('"')
                        .next()?
                        .parse()
                        .ok()
                };
                Some(number("x")? + number("width")? / 2.0)
            })
            .fold(f32::MIN, f32::max);
        assert!(note <= c - 5.0, "{note} {c}");
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
            ("A -> B : <img:x.png>", Some(0), "<img:x.png>"),
            ("participant A #red/blue", Some(0), "gradients"),
            ("title alone", None, "nothing but its title"),
            ("skinparam shadowing false", None, "the diagram is empty"),
            ("", None, "the diagram is empty"),
            ("== Part ==", None, "needs a participant"),
            ("note over A\n== head\nend note", Some(1), "headings"),
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

    #[test]
    fn monochrome_and_colors() {
        let svg = crate::render("participant A #ff0000\nA -[#00ff00]> A").unwrap();
        assert!(svg.contains("#FF0000") && svg.contains("#00FF00"));
        let svg =
            crate::render("skinparam monochrome true\nparticipant A #ff0000\nA -> A").unwrap();
        assert!(!svg.contains("#FF0000") && svg.contains("#4D4D4D"));
    }
}
