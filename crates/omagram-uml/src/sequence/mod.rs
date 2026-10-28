//! Sequence diagrams: the syntax tree, its parser and its drawing.
//!
//! The tree is flat and in source order, the way the diagram is drawn from
//! top to bottom: messages, notes, group starts and ends, activations.
//! Every item keeps its zero-based source line.

mod draw;
mod mermaid;
mod parse;
mod skin;

pub use crate::common::{Frame, Skinparam, Text};
pub use draw::{draw, draw_mapped};
pub use mermaid::{from_mermaid, mermaid_refusal};
pub use parse::parse;

/// A parsed sequence diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// In the order of their columns, left to right.
    pub participants: Vec<Participant>,
    pub items: Vec<Item>,
    /// `box … end box` around participants.
    pub boxes: Vec<ParticipantBox>,
    /// Participants drawn again below the lifelines (`hide footbox` turns
    /// it off).
    pub footbox: bool,
    /// `hide unlinked`: participants no message reaches are left out.
    pub hide_unlinked: bool,
    /// Title, header, footer, caption, legend, main frame, scale and
    /// skinparams.
    pub frame: Frame,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Participant {
    /// The name messages use: the alias if there is one.
    pub code: String,
    /// What the head shows.
    pub label: String,
    pub shape: Shape,
    pub stereotype: Option<String>,
    /// `<< (C,#ADD1B2) >>`: a letter in a colored circle beside the
    /// stereotype.
    pub spot: Option<Spot>,
    pub color: Option<String>,
    /// Where it was declared, or first used.
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spot {
    pub letter: char,
    /// As written; PlantUML's own green when there is none.
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Participant,
    Actor,
    Boundary,
    Control,
    Entity,
    Database,
    Collections,
    Queue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParticipantBox {
    pub title: Option<Text>,
    pub color: Option<String>,
    pub line: usize,
    /// Indexes into [`Diagram::participants`].
    pub participants: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Message(Message),
    Note(Note),
    /// `alt`, `loop`, … up to the matching [`Item::GroupEnd`].
    GroupStart {
        kind: GroupKind,
        label: Option<Text>,
        /// `group label [secondary]`.
        secondary: Option<Text>,
        line: usize,
    },
    /// `else`: the next part of the group around it.
    GroupElse {
        label: Option<Text>,
        line: usize,
    },
    GroupEnd {
        line: usize,
    },
    /// Mermaid `rect`: a background behind the following items, up to
    /// [`Item::RectEnd`]. Not a group, so it has no tab and no label.
    RectStart {
        color: String,
        line: usize,
    },
    RectEnd {
        line: usize,
    },
    /// `== label ==`.
    Divider {
        label: Text,
    },
    /// `...` or `... label ...`.
    Delay {
        label: Option<Text>,
        line: usize,
    },
    /// `|||` (one step of space) or `||45||` (45 pixels).
    Space {
        height: Option<f32>,
        line: usize,
    },
    /// `ref over A, B : text`.
    Ref {
        over: Vec<usize>,
        text: Text,
        color: Option<String>,
    },
    Activate {
        participant: usize,
        color: Option<String>,
        line: usize,
    },
    Deactivate {
        participant: usize,
        line: usize,
    },
    /// The lifeline ends here with a cross.
    Destroy {
        participant: usize,
        line: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    Alt,
    Opt,
    Loop,
    Par,
    Break,
    Critical,
    Group,
}

impl GroupKind {
    pub fn keyword(self) -> &'static str {
        match self {
            GroupKind::Alt => "alt",
            GroupKind::Opt => "opt",
            GroupKind::Loop => "loop",
            GroupKind::Par => "par",
            GroupKind::Break => "break",
            GroupKind::Critical => "critical",
            GroupKind::Group => "group",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub from: End,
    pub to: End,
    pub arrow: Arrow,
    pub text: Option<Text>,
    /// The `autonumber` label, markup included (`<b>3</b>`).
    pub number: Option<String>,
    /// It creates its receiver (`create` before it, `**` on it): the
    /// receiver's head is drawn at this arrow, not at the top.
    pub creates: bool,
    pub line: usize,
}

/// One end of a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
    Participant(usize),
    /// `[` or `]`: the edge of the diagram.
    Edge(Side),
    /// `?`: a short arrow beside the other end.
    Short(Side),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arrow {
    pub dashed: bool,
    /// The head at the receiving end.
    pub head: Head,
    /// A second head at the sending end (`<->`).
    pub tail: Head,
    /// `x` or `o` at the receiving end.
    pub end_mark: Mark,
    /// `x` or `o` at the sending end.
    pub start_mark: Mark,
    pub color: Option<String>,
    pub bold: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Head {
    #[default]
    None,
    /// `>`
    Filled,
    /// `>>`
    Open,
    /// `\` pointing right, `/` pointing left: the upper half only.
    UpperFilled,
    /// `/` pointing right, `\` pointing left.
    LowerFilled,
    /// `\\` pointing right, `//` pointing left.
    UpperOpen,
    /// `//` pointing right, `\\` pointing left.
    LowerOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mark {
    #[default]
    None,
    /// `x`: the message is lost.
    Cross,
    /// `o`
    Circle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub shape: NoteShape,
    pub place: NotePlace,
    pub text: Text,
    pub color: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteShape {
    /// `note`: a sheet with a folded corner.
    Note,
    /// `hnote`: a hexagon.
    Hexagon,
    /// `rnote`: a rectangle.
    Rectangle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotePlace {
    LeftOf(usize),
    RightOf(usize),
    /// Over one participant, or spanning from the first to the second.
    Over(usize, usize),
    /// Over all participants.
    Across,
    /// `note left` / `note right` right after a message: beside its arrow.
    Message(Side),
}
