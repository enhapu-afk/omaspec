//! Where a marble diagram's parts go, as the reference lays them out
//! (`marbleLayout.ts`), its words measured in the face they are drawn
//! in: a column every `columnWidth` from the right edge of the lanes'
//! names, the title and the axis above the first lane, and every lane in
//! a band as high as the fullest one needs, so the lanes stay evenly
//! apart. A label stands under its marble, or over the lane where it
//! would run into the one before, or further down with a line to its
//! marble.

use std::collections::HashMap;

use super::{Diagram, Item, Look};
use crate::chart::words;
use crate::text::{Block, Family, Font};

/// Room between the title and what follows it, and the axis's ticks and
/// the first lane.
const TITLE_GAP: f32 = 16.0;
const AXIS_GAP: f32 = 14.0;
/// Between two labels in a row.
const LABEL_GAP: f32 = 8.0;

/// The faces of a marble diagram's words.
pub(super) struct Fonts {
    pub title: Font,
    pub lane: Font,
    pub tick: Font,
    pub symbol: Font,
    pub label: Font,
}

impl Fonts {
    pub fn of(look: &Look) -> Fonts {
        let small = (look.size * 0.75).max(6.0);
        Fonts {
            title: Font::new(look.title_size, look.title).bold(),
            lane: Font::new((look.size * 0.875).max(6.0), look.lane_label),
            tick: Font::new(small, look.axis_text),
            symbol: Font::new(small, look.event_text).bold(),
            label: Font::new(small, look.label),
        }
    }
}

/// What a placed label says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Says {
    /// The words of the label of this index.
    Label(usize),
    /// A lane's time that passes off the scale: the item of this index.
    Time(usize),
}

/// Words beside a lane: in row 0 under it, in row 1 over it, in the rows
/// after under row 0 with a line up to their marble.
pub(super) struct Placed {
    pub says: Says,
    pub block: Block,
    /// Where its marble is, which its words stand centered under.
    pub x: f32,
    pub row: usize,
}

pub(super) struct LaidLane {
    pub y: f32,
    /// How far its marbles reach over and under its line: a radius, or
    /// half a stack.
    pub extent: f32,
    pub labels: Vec<Placed>,
    /// The items, by index, of the spans whose label stands inside them,
    /// in order.
    pub inside: Vec<usize>,
}

pub(super) struct Laid {
    pub fonts: Fonts,
    /// Where column 0 is.
    pub x0: f32,
    pub column_width: f32,
    /// The right edge of the lanes' names.
    pub names: f32,
    pub title: Option<Block>,
    /// The top of the axis's ticks.
    pub axis: Option<f32>,
    pub lanes: Vec<LaidLane>,
    /// Where the lanes start and end, which the grid runs between.
    pub top: f32,
    pub bottom: f32,
    /// The height of a label's row.
    pub row: f32,
    /// Each label's words, measured once.
    pub said: Vec<Block>,
    /// The label of each character that has one.
    pub by_symbol: HashMap<char, usize>,
}

impl Laid {
    pub fn x(&self, col: usize) -> f32 {
        self.x0 + col as f32 * self.column_width
    }

    /// The right end of the time.
    pub fn right(&self, columns: usize) -> f32 {
        self.x(columns.saturating_sub(1))
    }

    /// The top of a label's words in `row`, from its lane's line.
    pub fn row_top(&self, row: usize, extent: f32, height: f32) -> f32 {
        match row {
            0 => extent + 5.0,
            1 => -(extent + 6.0 + height),
            row => extent + 5.0 + (row - 1) as f32 * self.row,
        }
    }
}

/// How far right of its column a completion or an error in a `( )` group
/// stands, past the stack.
pub(super) fn grouped_shift(radius: f32) -> f32 {
    radius + 9.0
}

/// From the middle of one stacked marble to the next.
pub(super) fn pitch(radius: f32) -> f32 {
    2.0 * radius + 2.0
}

/// How far from the lane's line the marble `index` of a stack of `size`
/// stands.
pub(super) fn stacked(index: usize, size: usize, radius: f32) -> f32 {
    (index as f32 - (size as f32 - 1.0) / 2.0) * pitch(radius)
}

/// The label that says what `item` means, by its index among the labels.
pub(super) fn label_of(laid: &Laid, item: &Item) -> Option<usize> {
    let symbol = match item {
        Item::Event { symbol, .. } | Item::Span { symbol, .. } => *symbol,
        Item::Cancel { .. } => '!',
        Item::Error { .. } => '#',
        Item::Complete { .. } => '|',
        Item::Subscribe { .. } => '^',
        Item::Group { .. } | Item::Time { .. } => return None,
    };
    laid.by_symbol.get(&symbol).copied()
}

pub(super) fn lay(d: &Diagram, family: Family) -> Laid {
    let fonts = Fonts::of(&d.look);
    let config = &d.config;
    let radius = config.radius;
    let names = d
        .lanes
        .iter()
        .map(|lane| words(&lane.name, fonts.lane, family).width)
        .fold(0.0, f32::max);
    let x0 = names + config.label_gap + radius;
    let row = words("Xg", fonts.label, family).height + 4.0;
    let mut laid = Laid {
        x0,
        column_width: config.column_width,
        names,
        title: None,
        axis: None,
        lanes: Vec::new(),
        top: 0.0,
        bottom: 0.0,
        row,
        said: d
            .labels
            .iter()
            .map(|l| words(&l.text, fonts.label, family))
            .collect(),
        by_symbol: d.labels.iter().map(|l| (l.symbol, l.index)).collect(),
        fonts,
    };
    let mut y = 0.0;
    if let Some(title) = &d.title {
        let block = words(&title.value, laid.fonts.title, family).written_at(title.line, title.col);
        y += block.height + TITLE_GAP;
        laid.title = Some(block);
    }
    if !d.ticks.is_empty() {
        laid.axis = Some(y);
        y += words("0", laid.fonts.tick, family).height + AXIS_GAP;
    }
    laid.top = y;
    let mut lanes: Vec<LaidLane> = d
        .lanes
        .iter()
        .map(|lane| {
            let extent = lane
                .items
                .iter()
                .map(|item| match item {
                    Item::Group { size, .. } if *size > 1 => {
                        (*size - 1) as f32 * pitch(radius) / 2.0 + radius
                    }
                    _ => radius,
                })
                .fold(radius, f32::max);
            let (labels, inside) = place(d, lane, &laid, family);
            LaidLane {
                y: 0.0,
                extent,
                labels,
                inside,
            }
        })
        .collect();
    // Every lane in a band as high as the fullest needs.
    let mut above = radius + 6.0;
    let mut below = radius + 6.0;
    for lane in &lanes {
        let rows = lane.labels.iter().map(|l| l.row + 1).max().unwrap_or(0);
        let under = match rows {
            0 => 0,
            1 => 1,
            rows => rows - 1,
        };
        let over = if rows > 1 { row } else { 0.0 };
        above = above.max(lane.extent + 6.0 + over);
        below = below.max(lane.extent + 6.0 + under as f32 * row);
    }
    let band = config.lane_height.max(above + below);
    let offset = (band - above - below) / 2.0 + above;
    for lane in &mut lanes {
        lane.y = y + offset;
        y += band;
    }
    laid.lanes = lanes;
    laid.bottom = y;
    laid
}

/// A lane's labels, left to right, each in the first row where it keeps
/// clear of the one before; and the spans whose label fits inside them.
fn place(
    d: &Diagram,
    lane: &super::Lane,
    laid: &Laid,
    family: Family,
) -> (Vec<Placed>, Vec<usize>) {
    let config = &d.config;
    let mut inside = Vec::new();
    let mut wanted: Vec<Placed> = Vec::new();
    for (at, item) in lane.items.iter().enumerate() {
        if let Item::Time { col, text, byte } = item {
            wanted.push(Placed {
                says: Says::Time(at),
                block: words(text, laid.fonts.label, family).written_at(lane.line, *byte),
                x: laid.x(*col),
                row: 0,
            });
            continue;
        }
        let Some(index) = label_of(laid, item).filter(|&i| !d.labels[i].text.is_empty()) else {
            continue;
        };
        let block = laid.said[index].clone();
        let x = match item {
            Item::Span { from, to, .. } => {
                let (x1, x2) = (laid.x(*from), laid.x(*to));
                if block.width + 16.0 < x2 - x1 {
                    inside.push(at);
                    continue;
                }
                (x1 + x2) / 2.0
            }
            Item::Event { col, .. } | Item::Cancel { col } | Item::Subscribe { col } => {
                laid.x(*col)
            }
            Item::Complete { col, grouped } | Item::Error { col, grouped } => {
                laid.x(*col)
                    + if *grouped {
                        grouped_shift(config.radius)
                    } else {
                        0.0
                    }
            }
            Item::Group { .. } | Item::Time { .. } => continue,
        };
        if !config.event_labels {
            continue;
        }
        wanted.push(Placed {
            says: Says::Label(index),
            block,
            x,
            row: 0,
        });
    }
    wanted.sort_by(|a, b| a.x.total_cmp(&b.x));
    let mut ends: Vec<f32> = Vec::new();
    for placed in &mut wanted {
        let half = placed.block.width / 2.0;
        let row = ends
            .iter()
            .position(|&end| end <= placed.x - half - LABEL_GAP)
            .unwrap_or(ends.len());
        if row == ends.len() {
            ends.push(0.0);
        }
        ends[row] = placed.x + half;
        placed.row = row;
    }
    (wanted, inside)
}
