//! Salt: wireframes of windows and forms (`@startsalt`, or `salt` in an
//! `@startuml`), drawn as PlantUML draws them: a grid of widgets in thin
//! black lines on the page.
//!
//! What is read: grids (`{` without lines, `{+` framed, `{#` every line,
//! `{!` the lines between columns, `{-` those between rows), buttons
//! (`[OK]`), text fields (`"name"`), check boxes (`[X] label`, `[]`),
//! radio buttons (`(X) label`, `()`), drop-down lists (`^Pro^`), words,
//! an empty cell (`.`), rules (`--`, `==`, `..`, `~~`), a group box
//! (`{^"Title" … }`), tabs (`{/ a | b }`), a menu bar (`{* File | Edit }`)
//! and a tree (`{T + a ++ b }`), with columns (`+ World | 8 bn`) and a
//! grid's lines (`{T#`). A block may stand in a row's last cell
//! (`Files | {T`), the row going on after its close (`} | 3 files`). The
//! words take Creole and icons. Anything else declines and names the line.

mod draw;
mod parse;

pub use draw::draw;
pub use parse::{is_salt, parse};

use crate::common::Frame;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub root: Widget,
    pub frame: Frame,
}

/// Which lines a grid draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lines {
    None,
    /// `{+`: a frame around it.
    Frame,
    /// `{#`: every line.
    All,
    /// `{!`: between the columns.
    Columns,
    /// `{-`: between the rows.
    Rows,
}

/// A rule across a grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// `--`
    Line,
    /// `==`
    Double,
    /// `..`
    Dotted,
    /// `~~`
    Dashed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Widget {
    Grid {
        lines: Lines,
        rows: Vec<Row>,
    },
    /// `{^"Title" … }`: a frame with its title in its top line.
    Group {
        title: String,
        body: Box<Widget>,
    },
    /// `{/ a | b }`: the first is the one shown.
    Tabs(Vec<String>),
    /// `{* File | Edit }`.
    Menu(Vec<String>),
    /// `{T + a | x ++ b | y }`: its items, and the lines of a grid.
    Tree {
        lines: Lines,
        items: Vec<Item>,
    },
    Text(String),
    Button(String),
    Field(String),
    Check {
        on: bool,
        label: String,
    },
    Radio {
        on: bool,
        label: String,
    },
    Drop(String),
    Empty,
}

/// An item of a tree: how deep it stands, from 1, its words, and the
/// cells of the tree's other columns.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub depth: usize,
    pub label: String,
    pub cells: Vec<String>,
}

/// A row of a grid: its cells, or a rule across it.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Cells(Vec<Widget>),
    Rule(Rule),
}
