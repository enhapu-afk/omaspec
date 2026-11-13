//! Formulas (`@startlatex`, `@startmath`).
//!
//! One sheet, one expression. `@startlatex` is a TeX math subset and
//! `@startmath` is AsciiMath; both become the same boxes. A variable is a
//! serif italic, a digit or a function name is upright, and the fraction
//! bar, the radical and a parenthesis around a tall subformula are strokes.
//! There is no math font and no TeX: a command this module does not know
//! declines and names the line, and the Java PlantUML draws that whole sheet.
//!
//! A script takes one atom or one group, so `x^n+1` is `(x^n)+1`; a TeX
//! script takes one digit, so `x^23` is x² 3, as TeX reads it. `\mathrm`
//! is math in upright letters and `\text` is text. In
//! AsciiMath, a `/` takes one item on either side, as AsciiMath does: `1/2 m`
//! is ½·m, `ab/c` is a·(b/c), `sin x/2` is (sin x)/2, the brackets around
//! either side are not drawn, and a `/` right after a fraction is a slash.
//! AsciiMath reads the longest name first, so `sinx` is sin x and `xin` is
//! ξ n; a name of PlantUML's AsciiMath that this module does not draw
//! declines instead of turning into letters.
//!
//! A bracket closes with a bracket of either kind, so `[0, 1)` is one
//! fence. TeX pairs no brackets, so one that nothing closes on its level,
//! as in `]2, 3[`, is drawn alone, and a TeX `|` or `/` is a symbol. An
//! AsciiMath `|` that a second one closes on its level is an absolute
//! value, and any other is a bar, as in `P(A|B)`.

mod layout;
mod nodes;
mod paint;
mod pairs;
mod parse;
mod symbols;
#[cfg(test)]
mod tests;
pub use paint::draw;
pub use parse::parse;

use crate::color::Color;
use crate::common::Frame;

const DISPLAY: f32 = 20.0;
const SCRIPT: f32 = 0.7;
const SCRIPT_FLOOR: f32 = 10.0;
/// A sum or an integral, in ems of the surrounding style.
const LARGE: f32 = 1.7;
const AXIS: f32 = 0.25;
const THIN: f32 = 0.15;
const MED: f32 = 0.22;
const THICK: f32 = 0.28;
const FENCE_RATIO: f32 = 1.15;
const STROKE: f32 = 1.5;
const MARGIN: f32 = 16.0;
const NEST_LIMIT: u32 = 100;
const TITLE: f32 = 14.0;
const MUTED: Color = Color::hex(0x888888);

/// Which notation `parse` reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notation {
    /// `@startlatex`.
    Latex,
    /// `@startmath`.
    Ascii,
}

/// A parsed formula.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    root: Node,
    frame: Frame,
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Glyph(Glyph),
    Row(Vec<Node>),
    Frac(Box<Node>, Box<Node>),
    Sqrt {
        index: Option<Box<Node>>,
        body: Box<Node>,
    },
    Script {
        base: Box<Node>,
        sup: Option<Box<Node>>,
        sub: Option<Box<Node>>,
        limits: Limits,
    },
    Fence {
        open: Fence,
        body: Box<Node>,
        close: Fence,
    },
    /// A width in ems. Negative pulls the next atom back (`\!`).
    Space(f32),
}

#[derive(Debug, Clone, PartialEq)]
struct Glyph {
    text: String,
    atom: Atom,
    upright: bool,
    /// `1` for an ordinary symbol, [`LARGE`] for a display operator.
    scale: f32,
    limits: Limits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Atom {
    Ord,
    Op,
    Bin,
    Rel,
    Open,
    Close,
    Punct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Limits {
    /// A superscript beside the base, as on an integral.
    Side,
    /// Limits above and below, as on a sum.
    Above,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fence {
    Paren,
    Bracket,
    Brace,
    Bar,
    Double,
    None,
}

fn atom_class(node: &Node) -> Atom {
    match node {
        Node::Glyph(g) => g.atom,
        Node::Script { base, .. } => atom_class(base),
        _ => Atom::Ord,
    }
}
