//! Railroad diagrams: EBNF grammars (`@startebnf`) and regular expressions
//! (`@startregex`), drawn as PlantUML draws them, by one drawer. A rule
//! is a track from an open circle to a dot; a name it refers to stands
//! in a gray box, a word as written in a small white one, a special
//! sequence and a class of characters in a dashed one. Choices branch
//! below the first way, something that may be left out runs below a
//! straight track, and a repetition loops back over itself.

mod draw;
pub mod ebnf;
pub mod regex;

pub use draw::draw;

use crate::common::Frame;

/// A grammar or an expression to draw, rule by rule.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    /// Each rule with its name; a regular expression has one without.
    pub rules: Vec<(Option<String>, Node)>,
    pub frame: Frame,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// Words as written: `"+"`, a character.
    Terminal(String),
    /// A rule by its name.
    NonTerminal(String),
    /// `? 0 to 9 ?`, `any character`: said in words.
    Special(String),
    /// `[a-z0-9_]`: the characters a class takes, one under another; a
    /// class that takes all but them says so.
    Class {
        items: Vec<String>,
        negated: bool,
    },
    Sequence(Vec<Node>),
    /// The first way on the track, the others below it.
    Choice(Vec<Node>),
    /// May be left out: a straight track over it.
    Optional(Box<Node>),
    /// Once or more, with what the loop says (`{2,}`).
    Repeat {
        node: Box<Node>,
        label: Option<String>,
    },
    Empty,
}

impl Node {
    /// `nodes` as one: a sequence of several, the one itself, or nothing.
    pub fn sequence(mut nodes: Vec<Node>) -> Node {
        match nodes.len() {
            0 => Node::Empty,
            1 => nodes.remove(0),
            _ => Node::Sequence(nodes),
        }
    }

    /// `ways` as one: a choice of several, the one itself.
    pub fn choice(mut ways: Vec<Node>) -> Node {
        match ways.len() {
            0 => Node::Empty,
            1 => ways.remove(0),
            _ => Node::Choice(ways),
        }
    }

    /// Zero or more of `node`.
    pub fn any(node: Node) -> Node {
        Node::Optional(Box::new(Node::Repeat {
            node: Box::new(node),
            label: None,
        }))
    }
}
