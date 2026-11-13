//! Building the nodes of a formula and asking what they are: a glyph, a
//! row, an operand, a bracket, a matrix.

use super::parse::Stop;
use super::symbols::{ASCII_FUNCTIONS, Sym};
use super::{Atom, Fence, Glyph, Limits, Node, atom_class};

pub(super) fn unclosed(stop: Stop) -> String {
    match stop {
        Stop::Brace => "a `{` has no closing `}`".into(),
        Stop::Bracket => "a `[` has no closing `]`".into(),
        Stop::Close(open) => format!("a `{open}` has no closing bracket"),
        Stop::Bar => "a `|` has no closing `|`".into(),
        Stop::TexBrace => "a `\\{` has no closing `\\}`".into(),
        Stop::Right => "a `\\left` has no `\\right`".into(),
        Stop::End => "a formula needs an expression".into(),
    }
}

pub(super) fn plain(text: &str, atom: Atom, upright: bool) -> Node {
    Node::Glyph(glyph(text, atom, upright))
}

pub(super) fn glyph(text: &str, atom: Atom, upright: bool) -> Glyph {
    Glyph {
        text: text.to_string(),
        atom,
        upright,
        scale: 1.0,
        limits: Limits::Side,
    }
}

pub(super) fn from_sym(sym: Sym) -> Node {
    Node::Glyph(Glyph {
        text: sym.text.to_string(),
        atom: sym.atom,
        upright: sym.upright,
        scale: sym.scale,
        limits: sym.limits,
    })
}

pub(super) fn join(mut items: Vec<Node>) -> Node {
    if items.len() == 1 {
        items.remove(0)
    } else {
        Node::Row(items)
    }
}

pub(super) fn limits_of(node: &Node) -> Limits {
    match node {
        Node::Glyph(g) => g.limits,
        _ => Limits::Side,
    }
}

/// Latin letters turn upright; a lowercase Greek letter stays italic, as
/// in TeX.
pub(super) fn force_upright(node: Node) -> Node {
    match node {
        Node::Glyph(mut g) => {
            g.upright |= g.text.is_ascii();
            Node::Glyph(g)
        }
        Node::Row(items) => Node::Row(items.into_iter().map(force_upright).collect()),
        Node::Frac(a, b) => Node::Frac(Box::new(force_upright(*a)), Box::new(force_upright(*b))),
        Node::Sqrt { index, body } => Node::Sqrt {
            index: index.map(|n| Box::new(force_upright(*n))),
            body: Box::new(force_upright(*body)),
        },
        Node::Script {
            base,
            sup,
            sub,
            limits,
        } => Node::Script {
            base: Box::new(force_upright(*base)),
            sup: sup.map(|n| Box::new(force_upright(*n))),
            sub: sub.map(|n| Box::new(force_upright(*n))),
            limits,
        },
        Node::Fence { open, body, close } => Node::Fence {
            open,
            body: Box::new(force_upright(*body)),
            close,
        },
        Node::Space(w) => Node::Space(w),
    }
}

/// The item before an AsciiMath `/`, with the function it is the argument
/// of, as in `sin x/2` and `f(x)/2`.
pub(super) fn numerator(items: &mut Vec<Node>) -> Option<Node> {
    if !items.last().is_some_and(is_operand) {
        return None;
    }
    let item = items.pop()?;
    let applies = items.last().is_some_and(|function| {
        is_function(function) || (is_letter_function(function) && is_paren(&item))
    });
    if applies {
        let function = items.pop()?;
        return Some(Node::Row(vec![function, item]));
    }
    Some(unbracket(item))
}

pub(super) fn is_operand(node: &Node) -> bool {
    !matches!(node, Node::Space(_))
        && !matches!(
            atom_class(node),
            Atom::Bin | Atom::Rel | Atom::Punct | Atom::Open | Atom::Close
        )
}

pub(super) fn is_minus(node: &Node) -> bool {
    matches!(node, Node::Glyph(g) if g.text == "−" && g.atom == Atom::Bin)
}

pub(super) fn is_paren(node: &Node) -> bool {
    matches!(
        node,
        Node::Fence {
            open: Fence::Paren,
            ..
        }
    )
}

/// An AsciiMath function such as `sin`, which takes the next item as its
/// argument; `lim`, `min` and `max` do not.
pub(super) fn is_function(node: &Node) -> bool {
    match node {
        Node::Glyph(g) => g.atom == Atom::Op && ASCII_FUNCTIONS.contains(&g.text.as_str()),
        Node::Script { base, .. } => is_function(base),
        _ => false,
    }
}

/// `f` and `g`, which AsciiMath reads as functions of a parenthesis after
/// them only.
pub(super) fn is_letter_function(node: &Node) -> bool {
    match node {
        Node::Glyph(g) => !g.upright && matches!(g.text.as_str(), "f" | "g"),
        Node::Script { base, .. } => is_letter_function(base),
        _ => false,
    }
}

/// AsciiMath drops the brackets around either side of a `/`, around a
/// script and around the argument of `sqrt`.
pub(super) fn unbracket(node: Node) -> Node {
    match node {
        Node::Fence {
            open: Fence::Paren | Fence::Bracket | Fence::Brace,
            body,
            close: Fence::Paren | Fence::Bracket | Fence::Brace,
        } => *body,
        other => other,
    }
}

/// AsciiMath reads rows in the same brackets, separated by commas inside
/// a bracket, as a matrix: `((a,b),(c,d))`.
pub(super) fn is_matrix(items: &[Node]) -> bool {
    let Some(Node::Fence {
        open: open @ (Fence::Paren | Fence::Bracket),
        ..
    }) = items.first()
    else {
        return false;
    };
    items.len() >= 3
        && items.iter().enumerate().all(|(i, item)| match item {
            Node::Fence { open: o, .. } => i % 2 == 0 && o == open,
            Node::Glyph(g) => i % 2 == 1 && g.text == ",",
            _ => false,
        })
}
