//! Mermaid's marble diagrams (`marble-beta`, mermaid-js/mermaid#8864):
//! lanes of events over time, as ReactiveX explains its operators, read
//! as the proposal's reference reads them (REQ-105).
//!
//! Each lane is written in RxJS's marble syntax after its name and a
//! colon, one column of the source for each frame of time: `-` time
//! passes, any other character is a value, `|` completes the lane, `#`
//! errors it, `(abc)` holds values of one frame, `^` subscribes, `!`
//! unsubscribes, `9ms` is time that passes off the scale. The proposal
//! adds spans (`[===]`, `[~~>`, `S~~~E`) for what lasts, `/` for a break
//! in time on every lane, `axis : 0  5  10` for ticks at their columns,
//! `title`, and `label c "text"`, which says what a character means and
//! gives it a color of its own. Lanes measure from one origin, the
//! leftmost marble of any of them, so lanes lined up in the source line
//! up in the picture. What Mermaid refuses (a group never closed, a lane
//! written twice, a tab before a lane's first marble) is an error on its
//! line ([`mermaid_wrong`]).
//!
//! Read in [`read`], each lane's string in [`lane`], laid out in
//! [`layout`] and drawn in [`draw`].

mod draw;
mod lane;
mod layout;
mod read;

pub use draw::draw;
pub use read::{from_mermaid, mermaid_wrong};

use crate::color::{Color, Paint};
use crate::common::Text;

/// The settings of Mermaid's `marble` section Omaspec carries out, as
/// they are named there.
pub const SETTINGS: [&str; 6] = [
    "columnWidth",
    "laneHeight",
    "marbleRadius",
    "laneLabelGap",
    "padding",
    "showEventLabels",
];

/// The theme variables of a marble diagram, as Mermaid names them.
pub const COLORS: [&str; 17] = [
    "marbleTitleColor",
    "marbleLaneColor",
    "marbleLaneLabelColor",
    "marbleAxisTextColor",
    "marbleGridColor",
    "marbleEventFill",
    "marbleEventStroke",
    "marbleEventTextColor",
    "marbleLabelColor",
    "marbleSpanFill",
    "marbleSpanStroke",
    "marbleSpanTextColor",
    "marbleCompleteColor",
    "marbleErrorColor",
    "marbleCancelColor",
    "marbleBreakColor",
    "marbleBreakBkg",
];

/// A marble diagram as read.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    /// The axis's ticks, each at the column of its first character.
    pub ticks: Vec<Tick>,
    /// In the order written, top to bottom.
    pub lanes: Vec<Lane>,
    /// In the order written, which is the order of their colors.
    pub labels: Vec<Label>,
    /// The columns of a `/`: time is cut there, on every lane.
    pub breaks: Vec<usize>,
    /// From the origin to the end of the longest lane or the axis.
    pub columns: usize,
    pub look: Look,
    pub config: Config,
}

/// A tick of the axis, at its column, its words written at `byte` of its
/// line.
#[derive(Debug, Clone, PartialEq)]
pub struct Tick {
    pub col: usize,
    pub text: String,
    pub line: usize,
    pub byte: usize,
}

/// A lane: its name and what happens on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Lane {
    pub name: String,
    pub line: usize,
    /// The byte its name starts at.
    pub byte: usize,
    pub items: Vec<Item>,
    /// The stretches of columns it is active in, drawn as its line.
    pub segments: Vec<(usize, usize)>,
}

/// How a span ends: closed by `]` or an event, cancelled by `!` or `#`, or
/// still running when the diagram ends (`>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Close,
    Cancel,
    Open,
}

/// What happens on a lane, at its column.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A value. `stack` is its place in a `( )` group and the group's
    /// size; `byte` where its character stands on the lane's line.
    Event {
        symbol: char,
        col: usize,
        stack: Option<(usize, usize)>,
        byte: Option<usize>,
    },
    /// Something that lasts from one column to another, filled with
    /// `symbol` (`=` or `~`); a subscription lane (`--^---!`) is one span
    /// of `^`.
    Span {
        symbol: char,
        from: usize,
        to: usize,
        end: End,
        subscription: bool,
    },
    /// `|`, `#` and `!`: `grouped` inside a `( )` group, in its frame.
    Complete {
        col: usize,
        grouped: bool,
    },
    Error {
        col: usize,
        grouped: bool,
    },
    Cancel {
        col: usize,
    },
    /// `^`.
    Subscribe {
        col: usize,
    },
    /// A `( )` group: from the column of `(` to that of `)`, with `size`
    /// values.
    Group {
        col: usize,
        to: usize,
        size: usize,
    },
    /// Time that passes off the scale (`9ms`), drawn as a break with its
    /// words.
    Time {
        col: usize,
        text: String,
        byte: Option<usize>,
    },
}

/// `label c "text"`: what `symbol` means, and its color, in the order the
/// labels are written.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub symbol: char,
    pub text: String,
    pub line: usize,
    /// The byte its words start at.
    pub byte: usize,
    pub index: usize,
}

/// The sizes Mermaid's `marble` section sets, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Config {
    /// From one column to the next.
    pub column_width: f32,
    /// The least height of a lane; all grow together where labels need
    /// more room.
    pub lane_height: f32,
    pub radius: f32,
    /// Between a lane's name and its line.
    pub label_gap: f32,
    /// Around the picture.
    pub padding: f32,
    /// The labels' words beside the marbles; a span's inside it stay.
    pub event_labels: bool,
}

impl Config {
    /// Sets the number `key` of Mermaid's `marble` section to `value`,
    /// where it is one of them and within what a picture can take
    /// (a column 4 to 200 wide, a marble's radius 2 to 50); whether it
    /// was.
    pub fn set(&mut self, key: &str, value: f64) -> bool {
        let (slot, low, high) = match key {
            "columnWidth" => (&mut self.column_width, 4.0, 200.0),
            "laneHeight" => (&mut self.lane_height, 10.0, 400.0),
            "marbleRadius" => (&mut self.radius, 2.0, 50.0),
            "laneLabelGap" => (&mut self.label_gap, 0.0, 200.0),
            "padding" => (&mut self.padding, 0.0, 200.0),
            _ => return false,
        };
        if !(low..=high).contains(&value) {
            return false;
        }
        *slot = value as f32;
        true
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            column_width: 24.0,
            lane_height: 56.0,
            radius: 10.0,
            label_gap: 16.0,
            padding: 16.0,
            event_labels: true,
        }
    }
}

/// The colors and the type, as the theme and its marble variables give
/// them.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    /// Mermaid's `look: handDrawn`.
    pub sketch: bool,
    /// The theme's size of words; the lanes' names, the ticks and the
    /// labels are drawn a step under it.
    pub size: f32,
    pub title_size: f32,
    pub page: Paint,
    pub title: Color,
    pub lane: Color,
    pub lane_label: Color,
    pub axis_text: Color,
    pub grid: Color,
    pub event_fill: Color,
    pub event_stroke: Color,
    pub event_text: Color,
    pub label: Color,
    pub span_fill: Color,
    pub span_stroke: Color,
    pub span_text: Color,
    pub complete: Color,
    pub error: Color,
    pub cancel: Color,
    pub break_mark: Color,
    /// The gap under a break's strokes; none on a page left open, where
    /// a gap of a color would show.
    pub break_ground: Option<Color>,
    /// The colors of the labelled characters, in turn.
    pub palette: Vec<Color>,
}

impl Default for Look {
    /// Mermaid's default theme.
    fn default() -> Self {
        let line = Color::hex(0x333333);
        Look {
            font_name: crate::FACE.into(),
            sketch: false,
            size: 16.0,
            title_size: 20.0,
            page: None,
            title: line,
            lane: line,
            lane_label: line,
            axis_text: line,
            grid: line,
            event_fill: Color::hex(0xECECFF),
            event_stroke: line,
            event_text: line,
            label: line,
            span_fill: Color::hex(0xFFFFDE),
            span_stroke: line,
            span_text: line,
            complete: line,
            error: Color::hex(0xD64550),
            cancel: line,
            break_mark: line,
            break_ground: None,
            palette: crate::chart::PALETTE.into_iter().map(Color::hex).collect(),
        }
    }
}

impl Look {
    /// The page, white where it is left open.
    pub fn ground(&self) -> Color {
        self.page.unwrap_or(Color::WHITE)
    }

    /// The color of the `index`th label.
    fn color(&self, index: usize) -> Color {
        if self.palette.is_empty() {
            return self.event_fill;
        }
        self.palette[index % self.palette.len()]
    }
}
