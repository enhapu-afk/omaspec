//! Class, component, use case, deployment and object diagrams, laid out
//! by [`crate::graph`]. A Mermaid `classDiagram` is read into the same
//! tree.
//!
//! Names belong to the whole diagram, as in PlantUML: a line may join two
//! elements that sit in different packages. The package that holds both
//! keeps that line as a [`Crossing`]; the packages it runs into are laid
//! out with that one, and it runs through their ranks like any other.

pub(crate) mod archimate;
pub(crate) mod c4;
mod draw;
mod hide;
mod mermaid;
mod parse;
pub mod requirement;
mod skin;
mod spot;

pub use crate::common::{Frame, Skinparam, Text};
pub(crate) use draw::{ACTOR_FIGURE, paint_outline, words_area};
pub use draw::{draw, draw_mapped, label_by_hand};
pub use mermaid::{from_mermaid, mermaid_refusal};
pub use parse::{is_structure, parse};

/// Whether the diagram of `source` is a description diagram (components,
/// use cases, deployment) and no class diagram, as PlantUML tells the two
/// apart: its `title : x` is its title, its dotted names are whole.
pub fn described(source: &str) -> bool {
    crate::source::body(source)
        .is_ok_and(|(_, lines)| parse::described(&crate::common::deciding(&lines)))
}

/// A parsed structure diagram. The root is the page: it has no box.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub root: Group,
    pub frame: Frame,
}

/// The elements and lines written directly inside one package, node or
/// frame (or on the page). A nested package keeps its own group.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub nodes: Vec<Element>,
    pub edges: Vec<Link>,
    /// Lines whose ends lie in different boxes of this group.
    pub crossings: Vec<Crossing>,
    pub notes: Vec<Note>,
    pub direction: Direction,
    /// Its boxes the writer held beside others ([`Group::hold`]).
    pub held: Vec<crate::graph::hold::Held>,
    /// When false, no line here bundles
    /// ([0022](../../../docs/adr/0022-lines-bundled.md)).
    pub bundled: bool,
    /// Members whose lines stay out of a bundle.
    pub apart: Vec<usize>,
}

impl Diagram {
    /// Folds the packages, nodes and frames named in `names` to their box
    /// (docs/adr/0024); the lines into them end on its border.
    pub fn fold(&mut self, names: &[String]) {
        crate::fold::fold(&mut self.root, &mut self.frame, names);
    }
}

impl Group {
    /// How many elements and how many lines it holds, at any depth; a
    /// note of its own is no element.
    fn counts(&self) -> (usize, usize) {
        let mut elements = 0;
        // An association class's point is no element, and the line it
        // splits in two is one.
        let points = self.nodes.iter().filter(|n| n.shape == Shape::Point);
        let mut lines = (self.edges.len() + self.crossings.len()).saturating_sub(points.count());
        for node in &self.nodes {
            if !matches!(node.shape, Shape::Note | Shape::Point) {
                elements += 1;
            }
            let (e, l) = node.children.counts();
            elements += e;
            lines += l;
        }
        (elements, lines)
    }
}

impl crate::fold::Folds for Group {
    fn count(&self) -> usize {
        self.nodes.len()
    }

    fn key(&self, index: usize) -> &str {
        &self.nodes[index].key
    }

    fn composite(&self, index: usize) -> bool {
        self.nodes[index].composite
    }

    fn inner(&self, index: usize) -> &Group {
        &self.nodes[index].children
    }

    fn inner_mut(&mut self, index: usize) -> &mut Group {
        &mut self.nodes[index].children
    }

    fn close(&mut self, index: usize) {
        let node = &mut self.nodes[index];
        let (elements, lines) = node.children.counts();
        node.composite = false;
        node.children = Group {
            direction: node.children.direction,
            ..Group::default()
        };
        // What the box holds, where its stereotype would stand.
        node.stereotypes.push(crate::common::text_at(
            &format!(
                "{} · {}",
                crate::fold::counted(elements, "element"),
                crate::fold::counted(lines, "line")
            ),
            node.line,
        ));
    }

    fn crossings(&mut self) -> Vec<(&mut Vec<usize>, &mut Vec<usize>)> {
        self.crossings
            .iter_mut()
            .map(|c| (&mut c.from, &mut c.to))
            .collect()
    }

    fn settle(&mut self) {
        // Alike but for the line they were written on.
        let alike = |a: &Link, b: &Link| {
            Link {
                line: 0,
                ..a.clone()
            } == Link {
                line: 0,
                ..b.clone()
            }
        };
        let (direct, crossing): (Vec<Crossing>, Vec<Crossing>) =
            std::mem::take(&mut self.crossings)
                .into_iter()
                .partition(|c| c.from.len() == 1 && c.to.len() == 1);
        for c in crossing {
            if !self
                .crossings
                .iter()
                .any(|k| k.from == c.from && k.to == c.to && alike(&k.arrow, &c.arrow))
            {
                self.crossings.push(c);
            }
        }
        for c in direct {
            let edge = Link {
                from: c.from[0],
                to: c.to[0],
                ..c.arrow
            };
            if !self.edges.iter().any(|e| alike(e, &edge)) {
                self.edges.push(edge);
            }
        }
    }
}

impl Group {
    /// Gives each group the boxes of `held` it holds both of
    /// ([0021](../../../docs/adr/0021-boxes-held-in-place.md)), by their
    /// keys; returns the entries that name no box, or two in different
    /// groups.
    pub fn hold(&mut self, held: &[(usize, crate::hand::Hold)]) -> Vec<(usize, String)> {
        crate::hand::boxes::hold(self, held)
    }

    /// Keeps the lines of `entries` apart
    /// ([0022](../../../docs/adr/0022-lines-bundled.md)). An entry that
    /// names no box is returned, to be said.
    pub fn part(&mut self, entries: &[(usize, crate::hand::Apart)]) -> Vec<(usize, String)> {
        crate::hand::boxes::part(self, entries)
    }
}

impl crate::hand::boxes::Members for Group {
    fn count(&self) -> usize {
        self.nodes.len()
    }

    fn key(&self, index: usize) -> &str {
        &self.nodes[index].key
    }

    fn inner(&self, index: usize) -> &Group {
        &self.nodes[index].children
    }

    fn inner_mut(&mut self, index: usize) -> &mut Group {
        &mut self.nodes[index].children
    }

    fn kept(&mut self) -> crate::hand::boxes::Kept<'_> {
        crate::hand::boxes::Kept {
            held: &mut self.held,
            bundled: &mut self.bundled,
            apart: &mut self.apart,
        }
    }
}

impl Default for Group {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            crossings: Vec::new(),
            notes: Vec::new(),
            direction: Direction::Down,
            held: Vec::new(),
            bundled: true,
            apart: Vec::new(),
        }
    }
}

/// Which way this group's elements run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Down,
    Right,
}

/// One element: a class, a use case, a node, a package, …
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// The name lines use. An alias, when one was written.
    pub key: String,
    /// What the box shows.
    pub label: Text,
    pub stereotypes: Vec<Text>,
    pub members: Vec<Member>,
    pub shape: Shape,
    /// `#color` on the element, as written.
    pub fill: Option<String>,
    /// `##color` on the element, as written.
    pub line_color: Option<String>,
    /// Mermaid `stroke-width`. `None` uses the skin's thickness.
    pub thickness: Option<f32>,
    /// Mermaid `color`: the text on this box. `None` uses the skin.
    pub text_color: Option<String>,
    /// Mermaid `stroke-dasharray`, the on and off lengths of the border.
    pub border_dash: Option<(f32, f32)>,
    /// Set when the element was named with a keyword or a bracket form.
    /// A name a line invented can still take a real declaration.
    pub explicit: bool,
    /// `package Name { … }` or `node Name { … }`. An empty body is still
    /// a container. A class body is members, not a container.
    pub composite: bool,
    pub children: Group,
    /// `[[url]]` on its declaration (Mermaid: `click … href`, `link`): a
    /// click on the element follows it.
    pub link: Option<crate::text::Link>,
    /// `hide Name`: laid out and not drawn, nor its lines.
    pub hidden: bool,
    /// The circle before a class's name, in a class diagram; `hide
    /// circle` takes it away.
    pub spot: Option<Spot>,
    pub line: usize,
}

/// The circle PlantUML draws before a class's name: its kind's letter in
/// its kind's color (`C`, `I`, `A`, `E`, `@`), or a letter and a color of
/// its own (`<< (S,#FF7700) Singleton >>`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spot {
    pub kind: SpotKind,
    pub letter: char,
    /// As written; the kind's color when there is none.
    pub color: Option<String>,
    /// A stereotype wrote it (`<< (S,#FF7700) >>`): the box keeps it,
    /// declared again as another kind, until a stereotype is written again.
    pub own: bool,
}

impl Spot {
    /// The kind's own letter, in the kind's color.
    pub fn of(kind: SpotKind) -> Spot {
        Spot {
            kind,
            letter: kind.letter(),
            color: None,
            own: false,
        }
    }
}

/// What a class is, as its keyword says (`protocol`, `record`): each
/// kind's circle has a letter and a color of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpotKind {
    Class,
    Abstract,
    Interface,
    Enum,
    Annotation,
    Entity,
    Protocol,
    Struct,
    Exception,
    MetaClass,
    Record,
    DataClass,
}

impl SpotKind {
    pub const ALL: [SpotKind; 12] = [
        SpotKind::Class,
        SpotKind::Abstract,
        SpotKind::Interface,
        SpotKind::Enum,
        SpotKind::Annotation,
        SpotKind::Entity,
        SpotKind::Protocol,
        SpotKind::Struct,
        SpotKind::Exception,
        SpotKind::MetaClass,
        SpotKind::Record,
        SpotKind::DataClass,
    ];

    pub fn letter(self) -> char {
        match self {
            SpotKind::Class => 'C',
            SpotKind::Abstract => 'A',
            SpotKind::Interface => 'I',
            SpotKind::Enum | SpotKind::Entity => 'E',
            SpotKind::Annotation => '@',
            SpotKind::Protocol => 'P',
            SpotKind::Struct => 'S',
            SpotKind::Exception => 'X',
            SpotKind::MetaClass => 'M',
            SpotKind::Record => 'R',
            SpotKind::DataClass => 'D',
        }
    }

    /// The kind of a box of `shape` no keyword said more of.
    pub fn of_shape(shape: Shape) -> Option<SpotKind> {
        Some(match shape {
            Shape::Class => SpotKind::Class,
            Shape::Abstract => SpotKind::Abstract,
            Shape::Interface => SpotKind::Interface,
            Shape::Enum => SpotKind::Enum,
            Shape::Annotation => SpotKind::Annotation,
            Shape::Entity => SpotKind::Entity,
            _ => return None,
        })
    }
}

/// A field, a method, or a line between the two compartments.
#[derive(Debug, Clone, PartialEq)]
pub enum Member {
    Text {
        text: Text,
        italic: bool,
        underline: bool,
        slot: Slot,
    },
    /// `--` or `..` across the box.
    Rule(Dash),
    /// `== title ==`, centered between the fields.
    Heading(Text),
}

/// Which compartment a member belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Attribute,
    Method,
}

/// The picture of an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Class,
    Abstract,
    Interface,
    Enum,
    /// `entity`, a class by another name: a table of a data model.
    Entity,
    Annotation,
    Component,
    UseCase,
    Actor,
    /// `person`: a round box under a round head, as C4 draws a person.
    Person,
    /// `queue`: a cylinder lying on its side.
    Queue,
    Node,
    Artifact,
    Cloud,
    Database,
    Folder,
    Frame,
    Rectangle,
    Object,
    Package,
    File,
    Card,
    Storage,
    Stack,
    Hexagon,
    Agent,
    /// A note with no element of its own (`note "…" as N`).
    Note,
    /// An interface as a component diagram draws it (`() "Name"`, or an
    /// `interface` without members beside components), or a class
    /// diagram's `circle`: a small circle, its name under it.
    Lollipop,
    /// `diamond D` or `<> D`: the point the lines of one association of
    /// several classes meet, a small diamond without its name, as
    /// PlantUML draws it.
    Diamond,
    /// The middle of a line an association class meets (`(A, B) .. C`),
    /// as PlantUML makes one: laid out as a box of no size, drawn as
    /// nothing. The line between the two runs through it, and the class's
    /// line starts there. Its key is the pair as written, `(A, B)`.
    Point,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Dash {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

/// What sits at one end of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Head {
    #[default]
    None,
    /// `>` or `<`.
    Arrow,
    /// `<|` or `|>`, the inheritance triangle.
    Triangle,
    /// `o`, an empty diamond.
    Diamond,
    /// `*`, a filled diamond.
    DiamondFilled,
    /// `#`, an empty square on the box's border.
    Square,
    /// `x`, a cross.
    Cross,
    /// `}` or `{`, a crow's foot: many.
    Crowfoot,
    /// `+`, a circle with a cross in it: the box nested in the other.
    CirclePlus,
    /// `^`, an empty triangle, narrower than `<|`.
    Caret,
}

/// `A "1" --> "*" B : label`.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub from: usize,
    pub to: usize,
    pub label: Option<Text>,
    pub from_card: Option<Text>,
    pub to_card: Option<Text>,
    pub from_head: Head,
    pub to_head: Head,
    pub color: Option<String>,
    pub dash: Dash,
    pub bold: bool,
    /// `-[hidden]-`: the line places its boxes and is not drawn.
    pub hidden: bool,
    /// `-right->`, `-left->` (`-down->`, `-up->` where the flow runs to
    /// the right): the line runs within a rank, its second box right
    /// after its first across the flow or right before it
    /// ([0027](../../../docs/adr/0027-a-line-within-a-rank.md)).
    pub level: Option<bool>,
    pub line: usize,
    /// The side of the line its label takes, where the writer chose one
    /// by hand ([`crate::hand::Label`]).
    pub label_before: Option<bool>,
    /// `note on link`: a note by its words.
    pub note: Option<LinkNote>,
    /// `Foo::field1 --> Bar::field3`: the member each end is at, by its
    /// place among its element's members, as notes name them; the line
    /// meets its box at that member's row.
    pub members: [Option<usize>; 2],
    /// What its own style sets beyond its color, dash and boldness.
    pub style: LineStyle,
}

/// A line's width (`-[thickness=4]->`) and its words' color (`A --> B
/// #line:red;text:red`), as written.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LineStyle {
    pub thickness: Option<f32>,
    pub text: Option<String>,
}

/// A note on a line (`note on link`, `note right on link #pink`), as
/// PlantUML draws one: by the line's words, on the side written, under
/// them where none is; in their place where the line has none.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkNote {
    pub side: Option<Side>,
    pub text: Text,
    /// `#color` after `link`, as written.
    pub fill: Option<String>,
}

/// A line whose ends lie in different boxes of one group. The paths lead
/// from this group down to each end; `arrow.from` and `arrow.to` are
/// their first steps, the boxes here that hold the ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Crossing {
    pub from: Vec<usize>,
    pub to: Vec<usize>,
    pub arrow: Link,
}

pub use crate::graph::open::NoteSide as Side;

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub target: usize,
    /// The member of its element it is about (`note right of A::count`),
    /// by its index among the element's members; a dashed line leads to
    /// it.
    pub member: Option<usize>,
    pub side: Side,
    pub text: Text,
    pub line: usize,
    /// Its own fill (`note left of Foo #pink`), which may be a gradient,
    /// and its border's color (`##red`), as written.
    pub fill: Option<String>,
    pub line_color: Option<String>,
}
