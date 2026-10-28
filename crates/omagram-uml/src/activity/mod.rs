//! Activity diagrams in PlantUML's new syntax: actions, `if`, `while`,
//! `repeat`, `fork` and `split`, partitions, notes, arrow labels and
//! swimlanes.
//!
//! The tree is nested the way the source is: a flow is a list of steps,
//! and a step that branches holds flows of its own. Every step keeps the
//! swimlane it was written in and its zero-based source line.

mod branch;
mod draw;
mod layout;
mod parse;
mod skin;
mod tile;
mod words;

pub use crate::common::{Frame, Skinparam, Text};
pub use draw::draw;
pub use parse::{is_activity, parse};

/// A parsed activity diagram.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// In the order they were first named, left to right; empty without
    /// swimlanes.
    pub lanes: Vec<Lane>,
    pub flow: Flow,
    pub frame: Frame,
}

/// `|Name|`, `|#color|Name|` or `|Name| shown`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane {
    pub name: String,
    /// What the title shows: the text after the second `|`, else the name.
    pub title: Text,
    pub color: Option<String>,
}

/// Steps one below the other, joined by arrows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Flow {
    pub steps: Vec<Step>,
    /// `-> label;` after the last step: the arrow that leaves the flow.
    pub out: Option<Arrow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub node: Node,
    /// An index into [`Diagram::lanes`]; 0 without swimlanes.
    pub lane: usize,
    /// `-> label;` before it: the arrow that leads to it.
    pub arrow: Option<Arrow>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Start,
    /// A circle with a dot: the end of everything.
    Stop,
    /// A circle with a cross: the end of this flow.
    End,
    /// `kill` and `detach`: the flow stops without a symbol.
    Detach,
    /// `:text;`
    Action {
        text: Text,
        color: Option<String>,
        shape: ActionShape,
        note: Option<Note>,
    },
    /// `if` with its `elseif`s, each a test and the flow it leads to,
    /// and the `else`.
    If {
        branches: Vec<Branch>,
        otherwise: Branch,
    },
    /// `while (test) is (yes)` … `endwhile (no)`.
    While {
        test: Text,
        yes: Option<Text>,
        body: Flow,
        out: Option<Text>,
    },
    /// `repeat` … `repeat while (test) is (yes) not (no)`.
    Repeat {
        body: Flow,
        backward: Option<Backward>,
        test: Option<Text>,
        yes: Option<Text>,
        out: Option<Text>,
    },
    /// `fork`, `split`: flows side by side.
    Parallel {
        kind: Parallel,
        flows: Vec<Flow>,
    },
    /// `partition name {` … `}` and `group name` … `end group`: a frame
    /// with its name in a tab.
    Group {
        title: Text,
        color: Option<String>,
        flow: Flow,
    },
}

/// `(label) backward :text; (label)`: an action on the way back of a
/// `repeat`, with the labels of the lines into it and out of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backward {
    pub text: Text,
    pub incoming: Option<Text>,
    pub outgoing: Option<Text>,
}

/// One way through an `if`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Branch {
    /// The test in the diamond; `None` for the `else`.
    pub test: Option<Text>,
    /// `then (label)`, `else (label)`: next to the arrow into the flow.
    pub label: Option<Text>,
    /// `(label) elseif`: on the arrow from the diamond before.
    pub incoming: Option<Text>,
    pub flow: Flow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parallel {
    /// `fork` … `end fork`: black bars above and below.
    Fork,
    /// `fork` … `end merge`: a bar above, a diamond below.
    Merge,
    /// `split` … `end split`: plain lines.
    Split,
}

/// `->`, `-[#red,dashed]-> label;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arrow {
    pub label: Option<Text>,
    pub color: Option<String>,
    pub style: LineStyle,
    pub line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineStyle {
    #[default]
    Plain,
    Dashed,
    Dotted,
    Bold,
}

/// `note left` or `note right` of an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub right: bool,
    pub text: Text,
    pub color: Option<String>,
}

/// How an action is drawn: PlantUML's SDL shapes (`; <<input>>` or the
/// mark that ends it, `:text<`) and UML's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActionShape {
    /// The rounded box.
    #[default]
    Plain,
    /// `<`, a notch on the right.
    Input,
    /// `>`, a point on the right.
    Output,
    /// `|`, a line inside either side.
    Procedure,
    /// `/`, leaning right.
    Save,
    /// `\`, leaning left.
    Load,
    /// `}`, braces for sides.
    Continuous,
    /// `]`, square corners.
    Task,
    /// UML's object node: square corners.
    Object,
    /// UML's accept event: a notch on the left.
    AcceptEvent,
    /// UML's send signal: a point on the right.
    SendSignal,
}
