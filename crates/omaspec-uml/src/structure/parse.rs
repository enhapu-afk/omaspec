//! From the body's lines to a [`Diagram`]. A line the parser does not
//! understand declines the whole diagram, naming that line.
//!
//! Names belong to the whole diagram, as in PlantUML. A name a line uses
//! before it is declared is created where the line is; a later declaration
//! of that name fills the same element in.

use super::{
    Crossing, Dash, Diagram, Direction, Element, Group, Head, Link, Member, Note, Shape, Side,
    Slot, Spot, SpotKind, Text,
};
use crate::Declined;
use crate::color::{Inline, InlineDash};
use crate::common::{
    Frame, block_text, brace_delta, keyword, snippet, squeezed_eq, take_link, text_at,
};
use crate::graph::across::meeting;
use crate::source::{Line, offset_in, starts_with_ci};
use std::collections::HashSet;

mod association;

use association::Association;

/// Whether the lines of an `@startuml` diagram are a class, component,
/// use case, deployment or object diagram.
///
/// A sequence keyword (`participant`, `alt`, …) wins, so a login with
/// `actor` and `database` stays a sequence diagram. An activity or a state
/// diagram is detected before this and wins when both match.
pub fn is_structure(lines: &[Line]) -> bool {
    let mut depth: usize = 0;
    let mut saw = false;
    for line in lines {
        let text = line.text;
        if is_note_header(text) {
            continue;
        }
        if depth > 0 {
            depth = depth.saturating_add_signed(brace_delta(text));
            continue;
        }
        if sequence_line(text) {
            return false;
        }
        if marks_structure(text) {
            saw = true;
        }
        depth = depth.saturating_add_signed(brace_delta(text));
    }
    saw
}

fn is_note_header(text: &str) -> bool {
    keyword(text, "note").is_some()
        || keyword(text, "rnote").is_some()
        || keyword(text, "hnote").is_some()
}

/// A line that only a sequence diagram writes. `actor` and `database` are
/// not here: a use case and a deployment diagram use them too. A keyword
/// before an arrow is a name, as PlantUML reads it: `Box <|-- Crate`
/// relates a class named Box, `Loop -> Bob` a participant named Loop.
fn sequence_line(text: &str) -> bool {
    if text.starts_with("==") || text.starts_with("|||") || text.starts_with("...") {
        return true;
    }
    if text.strip_prefix('[').is_some_and(inner_is_arrow) {
        return true;
    }
    [
        "participant",
        "autonumber",
        "activate",
        "deactivate",
        "destroy",
        "return",
        "alt",
        "opt",
        "loop",
        "par",
        "break",
        "critical",
        "box",
        "create",
    ]
    .iter()
    .any(|word| keyword(text, word).is_some() && find_arrow(text).is_none())
        || keyword(text, "ref")
            .is_some_and(|rest| keyword(rest, "over").is_some() || rest.is_empty())
}

fn marks_structure(text: &str) -> bool {
    // A use case's actor alone (`:First Actor:`, `:Actor: as A`), or
    // declared with one (`actor :Last actor: as P`), and a use case
    // whatever follows the word, a description over several lines too.
    if let Some(end) = actor_span(text) {
        let rest = text[end..].trim_start();
        if rest.is_empty()
            || keyword(rest, "as").is_some()
            || rest.starts_with("<<")
            || rest.starts_with('#')
        {
            return true;
        }
    }
    if keyword(text, "actor").is_some_and(|rest| rest.starts_with(':'))
        || keyword(text, "usecase").is_some()
        || keyword(text, "action").is_some_and(|rest| !rest.is_empty())
    {
        return true;
    }
    // A class with its visibility before it (`-class "Private" {`).
    if let Some(rest) = text.strip_prefix(['-', '#', '~', '+'])
        && ["class", "abstract", "interface", "enum"]
            .iter()
            .any(|word| keyword(rest, word).is_some())
    {
        return true;
    }
    if relation_shaped(text) {
        return true;
    }
    let lead = before_label(text);
    // `actor` and `database` are sequence participants as well. They are
    // drawn here once the diagram is a structure diagram, but a line of
    // only those does not make it one. `map`, `json` and `port` do: the
    // parser then declines them, and the Java PlantUML draws them.
    if let Some((shape, _, rest)) = leading_shape(&lead)
        && !rest.is_empty()
        && !matches!(shape, Shape::Actor | Shape::Database)
        // An entity is a participant of a sequence diagram too; one with
        // a body is a class.
        && (shape != Shape::Entity || rest.ends_with('{'))
    {
        return true;
    }
    if ["map", "json", "port", "portin", "portout", "archimate"]
        .iter()
        .any(|word| keyword(&lead, word).is_some_and(|rest| !rest.is_empty()))
    {
        return true;
    }
    // A sequence diagram has no `together`.
    if squeezed_eq(text, "together{") {
        return true;
    }
    if let Some(inner) = starts_wrapped(&lead, '(', ')')
        && inner != "*"
    {
        return true;
    }
    if ends_wrapped(&lead, '(', ')') {
        return true;
    }
    if let Some(inner) = starts_wrapped(&lead, '[', ']')
        && !inner_is_arrow(inner)
    {
        return true;
    }
    ends_wrapped(&lead, '[', ']') || relation_only(text) || super::hide::marks_structure(text)
}

/// A line between two names whose ends no message has: `#--`, `x--`,
/// `}--`, `+--`, `^--` of a class diagram and an entity relationship's
/// crow's feet (`}|..||`, `|o--o{`). The structure parser says which of
/// them it draws.
fn relation_shaped(text: &str) -> bool {
    let words: Vec<&str> = text.split_whitespace().collect();
    let [left, arrow, right, ..] = words[..] else {
        return false;
    };
    let named =
        |word: &str| word.starts_with(|c: char| c.is_alphanumeric() || c == '"' || c == '_');
    let body = arrow.trim_matches(|c: char| "<>|o*#x}{+^".contains(c));
    named(left)
        && named(right)
        && body.len() >= 2
        && (body.chars().all(|c| c == '-') || body.chars().all(|c| c == '.'))
        && body.len() < arrow.len()
        && arrow.chars().any(|c| "|o#x}{+^".contains(c))
        && !crate::sequence::reads_as_message(text)
}

/// A line between two names that no sequence diagram writes:
/// `Usuario <|-- Cliente`, `A *-- B`, `A .. B`. PlantUML tries the
/// sequence diagram first and reads such a line as a class diagram's.
fn relation_only(text: &str) -> bool {
    find_arrow(text).is_some()
        && !crate::sequence::reads_as_message(text)
        && relation(text, 0, 0).is_ok()
}

/// The part of a line before `: label`. `:User:` is a name, not a label.
fn before_label(text: &str) -> String {
    let mut quote = false;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    if let Some(end) = actor_span(text) {
        let mut out = String::from("A");
        out.push_str(&text[end..]);
        return before_label(&out);
    }
    for (i, (b, c)) in chars.iter().enumerate() {
        if *c == '"' {
            quote = !quote;
            continue;
        }
        if !quote && *c == ':' && chars.get(i + 1).is_none_or(|(_, n)| *n != ':') {
            return text[..*b].trim_end().to_owned();
        }
    }
    text.to_owned()
}

fn actor_span(text: &str) -> Option<usize> {
    let rest = text.strip_prefix(':')?;
    let end = rest.find(':')?;
    if end == 0 {
        return None;
    }
    Some(end + 2)
}

/// What `text` starts with between `open` and `close`: `Login` in
/// `(Login) -- User`.
fn starts_wrapped(text: &str, open: char, close: char) -> Option<&str> {
    let rest = text.trim().strip_prefix(open)?;
    let inner = &rest[..rest.find(close)?];
    (!inner.is_empty()).then_some(inner)
}

fn ends_wrapped(text: &str, open: char, close: char) -> bool {
    let text = text.trim_end();
    let Some(start) = text.rfind(open) else {
        return false;
    };
    if start == 0 {
        return false;
    }
    let inner = text[start + open.len_utf8()..].strip_suffix(close);
    let Some(inner) = inner else {
        return false;
    };
    let before = text.as_bytes()[start - 1];
    (before.is_ascii_whitespace() || matches!(before, b'>' | b'-' | b'.'))
        && !inner.is_empty()
        && inner != "*"
        && !inner_is_arrow(inner)
}

/// Whether what follows a `[` is an arrow, as in `[-> A` or `[o-> A`, not
/// a name, as in `[Order]`.
fn inner_is_arrow(inner: &str) -> bool {
    let inner = inner.trim_start();
    let inner = inner.strip_prefix(['o', 'O', 'x', 'X']).unwrap_or(inner);
    inner.starts_with(['-', '<'])
}

/// Parses a structure diagram from the lines [`crate::source::body`] gives.
pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut parser = Parser::default();
    // A description diagram's (components, use cases, deployment) names
    // stay whole, and its `title : x` is its title, as PlantUML reads it.
    if described(&crate::common::deciding(lines)) {
        parser.classes = false;
        parser.frame.colons = crate::common::Colons::Both;
    }
    let mut at = 0;
    while at < lines.len() {
        at = match parser.frame.line(lines, at)? {
            Some(next) => next,
            None => parser.line(lines, at)?,
        };
    }
    parser.finish()
}

const MAX_DEPTH: usize = 100;

/// Whether `lines` hold what only PlantUML's description diagrams have,
/// as PlantUML tells one from a class diagram: a component, a use case,
/// an actor, a node or their kin declared alone (`node N`; a block,
/// `node N {`, is a class diagram's package), or a box in brackets
/// (`[Web Server]`, `(Use it)`, `:Main Admin:`) alone on its line or at a
/// line's end, not in a label. A class called `Node` is no node
/// (`Node --> Edge`, `Node : name`). Under `allowmixing` a class diagram
/// holds them all.
pub(super) fn described(lines: &[Line]) -> bool {
    let mixing = lines.iter().any(|line| {
        squeezed_eq(line.text, "allowmixing") || squeezed_eq(line.text, "allow_mixing")
    });
    if mixing {
        return false;
    }
    /// What a class diagram never declares alone.
    const LEAVES: [&str; 26] = [
        "component",
        "usecase",
        "actor",
        "node",
        "cloud",
        "frame",
        "folder",
        "storage",
        "artifact",
        "card",
        "file",
        "stack",
        "hexagon",
        "agent",
        "queue",
        "database",
        "boundary",
        "control",
        "person",
        "collections",
        "port",
        "portin",
        "portout",
        "rectangle",
        "label",
        "archimate",
    ];
    /// What a class diagram takes as a package neither: no container.
    const NO_BLOCK: [&str; 11] = [
        "usecase",
        "actor",
        "agent",
        "boundary",
        "control",
        "person",
        "collections",
        "port",
        "portin",
        "portout",
        "label",
    ];
    lines.iter().any(|line| {
        let text = line.text.trim();
        let word: String = text
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect::<String>()
            .to_ascii_lowercase();
        let after = &text[word.len()..];
        // `Node : name` is a member; `actor :User:` an actor.
        let member = after
            .trim_start()
            .strip_prefix(':')
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace));
        let declares = LEAVES.contains(&word.as_str())
            && (after.is_empty()
                || after.starts_with(char::is_whitespace)
                || after.starts_with('/'))
            && !member
            && !names_an_end(text, &word);
        let keyword = declares && (!text.ends_with('{') || NO_BLOCK.contains(&word.as_str()));
        // A line's far end: past an arrow that a name stands before, not
        // one in a note's or a title's words (`note left : x -> (y)`).
        let far_end = find_arrow(text).is_some_and(|(start, end)| {
            let near = text[..start].trim_end();
            // Past the line's multiplicity at its first end.
            let near = match near.strip_suffix('"').and_then(|n| n.rsplit_once('"')) {
                Some((name, _)) => name.trim_end(),
                None => near,
            };
            let named = !near.is_empty()
                && (bracketed(near)
                    || !near.contains(char::is_whitespace)
                    || (near.starts_with('"') && near.ends_with('"')));
            if !named {
                return false;
            }
            let rest = text[end..].trim_start();
            // Past the line's multiplicity at its second end.
            let rest = match rest.strip_prefix('"').and_then(|r| r.split_once('"')) {
                Some((_, rest)) => rest.trim_start(),
                None => rest,
            };
            bracketed(rest)
        });
        keyword || bracketed(text) || far_end
    })
}

/// Whether `text` starts with a box in brackets: `[Web Server]` (not
/// `[*]` nor a link), `(Use it)` (not `(*)` nor a pair `(A, B)`), or
/// `:Main Admin:`.
fn bracketed(text: &str) -> bool {
    let inside = |open: char, close: char| {
        text.strip_prefix(open)
            .and_then(|rest| rest.split_once(close))
            .map(|(inside, _)| inside)
            .filter(|inside| !inside.trim().is_empty())
    };
    let component = !text.starts_with("[[") && inside('[', ']').is_some_and(|i| i != "*");
    let use_case = inside('(', ')').is_some_and(|i| !i.starts_with('*') && !i.contains(','));
    let actor = inside(':', ':').is_some_and(|i| !i.starts_with(char::is_whitespace));
    component || use_case || actor
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Open {
    None,
    Members,
    Group,
}

/// The word of a line `together` makes: across the flow, whichever way
/// the flow runs, the second box after the first.
const ACROSS: &str = "across";

/// The word of a line of one dash or dot and no word (`A <|- B`): its
/// second box beside its first, as `-right->` holds it, the two in the
/// order written, as PlantUML ranks them; between boxes of different
/// packages a line like any other.
const BESIDE: &str = "beside";

struct RawName {
    key: String,
    label: String,
    shape: Option<Shape>,
    /// Written in quotes: one name, whatever it holds (`"ns::Impl"`).
    quoted: bool,
}

struct Inherit {
    raw: RawName,
    dash: Dash,
}

struct Decl {
    key: String,
    label: String,
    shape: Shape,
    stereotypes: Vec<String>,
    /// The circle its keyword gives it (`protocol`: `P`), or the one its
    /// stereotype writes (`<< (S,#FF7700) >>`); else its shape's.
    spot: Option<Spot>,
    /// A stereotype wrote its circle.
    spotted: bool,
    fill: Option<String>,
    line_color: Option<String>,
    /// The rest of an inline style (`#fill;line.dashed;text:navy`).
    more: More,
    open: Open,
    parents: Vec<Inherit>,
    link: Option<crate::text::Link>,
    line: usize,
}

struct ArrowBits {
    from_head: Head,
    to_head: Head,
    dash: Dash,
    bold: bool,
    hidden: bool,
    /// The word of a hidden line that says where its second box goes
    /// (`-[hidden]right-`).
    toward: Option<&'static str>,
    color: Option<String>,
    /// Its width and its words' color, written in its bracket or after
    /// its far end.
    style: super::LineStyle,
}

struct RawRel {
    from: RawName,
    to: RawName,
    from_card: Option<Text>,
    to_card: Option<Text>,
    arrow: ArrowBits,
    label: Option<String>,
}

struct Pending {
    from: RawName,
    to: RawName,
    /// The members its ends are at (`Foo::field1 --> Bar::field3`), found
    /// on their boxes as the line is placed.
    members: MemberEnds,
    /// Its own width and words' color.
    style: super::LineStyle,
    /// `note on link` after it.
    note: Option<super::LinkNote>,
    from_card: Option<Text>,
    to_card: Option<Text>,
    from_head: Head,
    to_head: Head,
    dash: Dash,
    bold: bool,
    hidden: bool,
    toward: Option<&'static str>,
    color: Option<String>,
    label: Option<String>,
    line: usize,
    home: Vec<usize>,
    /// Whether each end names its box by its key alone: one a line names
    /// before any box of that name stood (PlantUML makes it then), or a
    /// declaration's parent (`extends Socket`), is not a class made from
    /// `net.Socket`.
    strict: [bool; 2],
    /// The box each end found where the line stands, by a short name
    /// (`C` of `a.b.C`): a `C` declared later is not the line's.
    found: [Option<Vec<usize>>; 2],
}

/// A line's ends at members: for each, whether the member is named by its
/// quoted words, and the name or the words.
type MemberEnds = [Option<(bool, String)>; 2];

struct RawNote {
    target: String,
    /// The package it is written in, where its target is looked for
    /// first.
    home: Vec<usize>,
    /// The box it is on, where the note named none and that box is known.
    at: Option<Vec<usize>>,
    side: Side,
    text: Text,
    line: usize,
    fill: Option<String>,
    line_color: Option<String>,
}

/// Under `!pragma useIntermediatePackages false`: each package made for a
/// name's parts that holds nothing but one package is that package, their
/// names joined (`a.b`), as PlantUML makes them; the lines that run
/// through it keep their ends.
/// Only the classes of a class diagram (or of one under `allowmixing`)
/// have a circle before their name, as PlantUML draws them: not an
/// interface drawn as a lollipop, nor anything of a description diagram.
fn keep_spots(group: &mut Group, classes: bool) {
    for node in &mut group.nodes {
        if !classes || SpotKind::of_shape(node.shape).is_none() {
            node.spot = None;
        }
        keep_spots(&mut node.children, classes);
    }
}

fn flatten(root: &mut Group, sep: &str) {
    // The first such package, by its path.
    fn chain(group: &Group, path: &mut Vec<usize>) -> Option<Vec<usize>> {
        for (index, node) in group.nodes.iter().enumerate() {
            path.push(index);
            let inner = &node.children;
            let only = inner.nodes.len() == 1
                && inner.nodes[0].composite
                && inner.nodes[0].shape == Shape::Package
                && inner.edges.is_empty()
                && inner.crossings.is_empty()
                && inner.notes.is_empty();
            if !node.explicit && node.composite && node.shape == Shape::Package && only {
                return Some(path.clone());
            }
            if let Some(found) = chain(&node.children, path) {
                return Some(found);
            }
            path.pop();
        }
        None
    }
    // The ends of the lines kept above `at` that run through it.
    fn mend(group: &mut Group, above: usize, at: &[usize]) {
        for crossing in &mut group.crossings {
            for end in [&mut crossing.from, &mut crossing.to] {
                let through = at.len() - above;
                if end.len() > through && end[..through] == at[above..] {
                    end.remove(through);
                }
            }
        }
    }
    while let Some(at) = chain(root, &mut Vec::new()) {
        // Every group from the top down to the one holding the package.
        let mut group = &mut *root;
        for depth in 0..at.len() - 1 {
            mend(group, depth, &at);
            group = &mut group.nodes[at[depth]].children;
        }
        mend(group, at.len() - 1, &at);
        let index = at[at.len() - 1];
        let outer = &mut group.nodes[index];
        let mut inner = outer.children.nodes.pop().expect("one package");
        inner.label.value = format!("{}{sep}{}", outer.label.value, inner.label.value);
        group.nodes[index] = inner;
    }
}

/// How a name finds its box ([`Parser::find_by`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lookup {
    /// By its key alone.
    Key,
    /// As a declaration finds it.
    Declared,
    /// As a line finds it.
    Line,
}

/// The label of a box made in packages from a whole name: its last part,
/// with what was written after the name (`<T>`), or the label written of
/// its own.
fn short_label(label: &str, key: &str, last: &str) -> String {
    match label.strip_prefix(key) {
        Some(rest) => format!("{last}{rest}"),
        None => label.to_owned(),
    }
}

/// What a parser made last ([`Parser::recent`]).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Recent {
    Nothing,
    /// A box by its name, and where it stands when that is known.
    Element(String, Option<Vec<usize>>),
    Note,
    /// An association's point (`(A, B) .. C`).
    Point,
}

struct Parser {
    frame: Frame,
    root: Group,
    open: Vec<(Vec<usize>, usize)>,
    members: Option<(Vec<usize>, usize)>,
    slot: Slot,
    last: Option<Vec<usize>>,
    /// What was made last, as PlantUML counts it for a note without `of`:
    /// an element declared or first named (by a line, by a member line),
    /// or a note.
    recent: Recent,
    /// The names declared or named so far, for [`Parser::recent`].
    named: HashSet<String>,
    pending: Vec<Pending>,
    raw_notes: Vec<RawNote>,
    /// The notes on members, resolved once `hide` has taken members and
    /// boxes out: by the line of each note, its member's name or quoted
    /// words.
    on_members: Vec<(usize, bool, String)>,
    /// What parts a class's name into the packages it stands in (`a.b.C`):
    /// `.`, or `set separator ::`; none after `set separator none`.
    separator: Option<String>,
    /// A class (or object) diagram, whose names part so; a description
    /// diagram's do not ([`described`]).
    classes: bool,
    notes_made: usize,

    /// The last `left to right direction` or `top to bottom direction`,
    /// which holds for the whole diagram, inside packages too, as in
    /// PlantUML.
    direction: Option<Direction>,
    /// `together { … }` blocks open: the boxes declared right in each,
    /// with its line and how many packages were open around it.
    together: Vec<(Vec<RawName>, usize, usize)>,
    /// `hide`, `show`, `remove` and `restore`, laid on the boxes once all
    /// are known.
    hides: Vec<super::hide::Directive>,
    /// Each `$tag` a declaration carries, by its box's path: a name may
    /// stand in two packages.
    tags: Vec<(Vec<usize>, String)>,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            // `title : x` is a member of a class named `title`.
            frame: Frame {
                colons: crate::common::Colons::Tight,
                ..Frame::styled(super::skin::STYLE)
            },
            root: Group::default(),
            open: Vec::new(),
            members: None,
            slot: Slot::Attribute,
            last: None,
            recent: Recent::Nothing,
            named: HashSet::new(),
            pending: Vec::new(),
            raw_notes: Vec::new(),
            on_members: Vec::new(),
            separator: Some(".".to_owned()),
            classes: true,
            notes_made: 0,

            direction: None,
            together: Vec::new(),
            hides: Vec::new(),
            tags: Vec::new(),
        }
    }
}

impl Parser {
    fn finish(mut self) -> Result<Diagram, Declined> {
        if let Some((_, line)) = self.members {
            return Err(Declined::at(line, "a declaration has no `}`"));
        }
        if let Some((_, line)) = self.open.last() {
            return Err(Declined::at(*line, "a package has no `}`"));
        }
        if let Some((_, line, _)) = self.together.last() {
            return Err(Declined::at(*line, "`together` has no `}`"));
        }
        let pending = std::mem::take(&mut self.pending);
        // `(A, B)` is an association class's pair where both name boxes
        // of the diagram, declared or at the end of a line.
        let named: HashSet<&str> = pending
            .iter()
            .flat_map(|item| [&item.from, &item.to])
            .filter(|raw| raw.pair().is_none())
            .map(|raw| raw.key.as_str())
            .collect();
        let known = |name: &str| named.contains(name) || self.find(name).is_some();
        let pairs: Vec<[bool; 2]> = pending
            .iter()
            .map(|item| {
                [&item.from, &item.to]
                    .map(|raw| raw.pair().is_some_and(|(a, b)| known(a) && known(b)))
            })
            .collect();
        let mut resolved = Vec::with_capacity(pending.len());
        let mut associations = Vec::new();
        for (item, pair) in pending.into_iter().zip(pairs) {
            match pair {
                [false, false] => {
                    let mut ends = Vec::with_capacity(2);
                    for (at, raw) in [&item.from, &item.to].into_iter().enumerate() {
                        ends.push(match &item.found[at] {
                            Some(path) => {
                                self.upgrade(path, raw, item.line)?;
                                path.clone()
                            }
                            None => self.ensure(raw, &item.home, item.line, item.strict[at])?,
                        });
                    }
                    let to = ends.pop().expect("two ends");
                    let from = ends.pop().expect("two ends");
                    resolved.push((from, to, item));
                }
                [true, true] => {
                    return Err(Declined::at(
                        item.line,
                        "a line between two association classes is not supported yet",
                    ));
                }
                [pair_first, _] => {
                    let (pair, class) = if pair_first {
                        (&item.from, &item.to)
                    } else {
                        (&item.to, &item.from)
                    };
                    let class = self.ensure(class, &item.home, item.line, false)?;
                    let (a, b) = pair.pair().expect("a pair");
                    associations.push(Association {
                        pair: (a.to_owned(), b.to_owned()),
                        class,
                        pair_first,
                        item,
                    });
                }
            }
        }
        for (from, to, item) in resolved {
            self.place_link(from, to, &item)?;
        }
        for association in associations {
            self.associate(association)?;
        }
        self.place_notes()?;
        lollipops(&mut self.root);
        keep_spots(&mut self.root, self.classes);
        let separator = self.separator.clone().filter(|_| self.classes);
        super::hide::apply(
            &mut self.root,
            &self.hides,
            &self.tags,
            separator.as_deref(),
        )?;
        self.resolve_members();
        if self.frame.flat_packages
            && let Some(sep) = &separator
        {
            flatten(&mut self.root, sep);
        }
        if let Some(direction) = self.direction {
            run_all(&mut self.root, direction);
        }
        Ok(Diagram {
            root: self.root,
            frame: self.frame,
        })
    }

    fn here(&self) -> Vec<usize> {
        self.open
            .last()
            .map(|(path, _)| path.clone())
            .unwrap_or_default()
    }

    fn container(&self, path: &[usize]) -> &Group {
        let mut group = &self.root;
        for &index in path {
            group = &group.nodes[index].children;
        }
        group
    }

    fn container_mut(&mut self, path: &[usize]) -> &mut Group {
        let mut group = &mut self.root;
        for &index in path {
            group = &mut group.nodes[index].children;
        }
        group
    }

    fn node_mut(&mut self, path: &[usize]) -> &mut Element {
        let (last, home) = path.split_last().expect("an element has a path");
        &mut self.container_mut(home).nodes[*last]
    }

    fn node(&self, path: &[usize]) -> &Element {
        let (last, home) = path.split_last().expect("an element has a path");
        &self.container(home).nodes[*last]
    }

    fn line(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line {
            no,
            text,
            at: start,
            ..
        } = lines[at];
        if self.members.is_some() {
            return self.member_line(text, no, at);
        }
        if squeezed_eq(text, "lefttorightdirection") {
            self.direction = Some(Direction::Right);
            return Ok(at + 1);
        }
        if squeezed_eq(text, "toptobottomdirection") {
            self.direction = Some(Direction::Down);
            return Ok(at + 1);
        }
        if squeezed_eq(text, "allowmixing") || squeezed_eq(text, "allow_mixing") {
            return Ok(at + 1);
        }
        // `set separator ::`, `set namespaceSeparator none`; a class named
        // `Set` is no such line.
        if let Some(value) = keyword(text, "set").and_then(|rest| {
            keyword(rest, "separator").or_else(|| keyword(rest, "namespaceSeparator"))
        }) {
            self.separator = match value.trim() {
                "" => return Err(Declined::at(no, "a separator is missing")),
                none if none.eq_ignore_ascii_case("none") => None,
                sep => Some(sep.to_owned()),
            };
            return Ok(at + 1);
        }
        // The boxes of `together` stand in one rank, side by side in the
        // order written: each held after the one before it, as a hidden
        // line across the flow holds it (docs/adr/0021).
        if squeezed_eq(text, "together{") {
            if self.together.len() >= MAX_DEPTH {
                return Err(Declined::at(no, "`together` nests too deep"));
            }
            self.together.push((Vec::new(), no, self.open.len()));
            return Ok(at + 1);
        }
        if text == "}" {
            if self
                .together
                .last()
                .is_some_and(|(_, _, depth)| *depth == self.open.len())
            {
                let (members, line, _) = self.together.pop().expect("a together block");
                let home = self.here();
                for pair in members.windows(2) {
                    self.pending.push(Pending {
                        members: [None, None],
                        style: Default::default(),
                        from: pair[0].clone_name(),
                        to: pair[1].clone_name(),
                        from_card: None,
                        to_card: None,
                        from_head: Head::None,
                        to_head: Head::None,
                        dash: Dash::Solid,
                        bold: false,
                        hidden: true,
                        toward: Some(ACROSS),
                        color: None,
                        label: None,
                        line,
                        home: home.clone(),
                        strict: [false, false],
                        found: [None, None],
                        note: None,
                    });
                }
                return Ok(at + 1);
            }
            self.close(no)?;
            return Ok(at + 1);
        }
        if text == "{" {
            self.open_last(no)?;
            return Ok(at + 1);
        }
        if squeezed_eq(text, "endnote") || squeezed_eq(text, "endrnote") {
            return Err(Declined::at(no, "there is no note for this end line"));
        }
        if let Some(verb) = super::hide::verb(text)
            && !names_an_end(text, verb)
            && let Some(directive) = super::hide::directive(text, no)?
        {
            self.hides.push(directive.written_in(self.here()));
            return Ok(at + 1);
        }
        if crate::common::description_opens(text) {
            return Err(Declined::at(
                no,
                "an element's words in brackets over several lines (`name [ … ]`) are not supported yet",
            ));
        }
        if let Some(word) = rejected(text) {
            let why = match word {
                "map" => "a `map` (a table of keys and values) is not drawn here yet".to_owned(),
                "json" => {
                    "a `json` element (JSON data in the diagram) is not drawn here yet".to_owned()
                }
                "together" => "`together` is not supported yet".to_owned(),
                _ => format!("`{word}` is not supported yet"),
            };
            return Err(Declined::at(no, why));
        }
        if text.contains("<<$") {
            return Err(Declined::at(
                no,
                "a sprite as a stereotype (`<<$name>>`) is not supported yet",
            ));
        }
        if is_note_header(text) {
            return self.note(lines, at);
        }
        let (untagged, tags) = if leading_shape(text).is_some() && text.contains('$') {
            super::hide::take_tags(text)
        } else {
            (String::new(), Vec::new())
        };
        let declared = if tags.is_empty() { text } else { &untagged };
        if let Some(decl) = declaration(declared, no)? {
            self.declare(decl)?;
            if let Some(path) = &self.last {
                self.tags
                    .extend(tags.into_iter().map(|tag| (path.clone(), tag)));
            }
            return Ok(at + 1);
        }
        if find_arrow(text).is_some() {
            self.add_relation(relation(text, no, start)?, no)?;
            return Ok(at + 1);
        }
        if let Some((name, member)) = field_line(text, no)? {
            self.add_field(name, member, no)?;
            return Ok(at + 1);
        }
        if let Some(decl) = bare(text, no)? {
            self.declare(decl)?;
            return Ok(at + 1);
        }
        Err(Declined::at(
            no,
            format!("`{}` is not supported yet", snippet(text)),
        ))
    }

    fn member_line(&mut self, text: &str, no: usize, at: usize) -> Result<usize, Declined> {
        if text == "}" {
            self.members = None;
            return Ok(at + 1);
        }
        if opens_declaration(text) {
            return Err(Declined::at(
                no,
                "a nested declaration is not supported yet",
            ));
        }
        let path = self.members.as_ref().expect("members").0.clone();
        if let Some(member) = member_from(text, no, &mut self.slot)? {
            self.node_mut(&path).members.push(member);
        }
        Ok(at + 1)
    }

    fn declare(&mut self, mut decl: Decl) -> Result<(), Declined> {
        let here = self.here();
        let path =
            if let Some((home, last)) = self.qualified(&decl.key, decl.shape, &here, decl.line)? {
                // `class a.b.C`: in the packages its name gives, called by
                // its last part.
                decl.label = short_label(&decl.label, &decl.key, &last);
                match self.child(&home, &decl.key, &last) {
                    Some(path) => {
                        self.merge(&path, &decl)?;
                        path
                    }
                    None => {
                        let label = decl.label.clone();
                        self.insert_at(&decl, &home, &label)?
                    }
                }
            } else if let Some(path) = if self.namespaced()
                && (SpotKind::of_shape(decl.shape).is_some()
                    || decl.open == Open::Group
                    || decl.shape == Shape::Package)
            {
                // A class and its kin, and a package or any box that holds
                // others, belong to the package they are declared in.
                self.find_near(&decl.key, Lookup::Declared, &here)
            } else {
                self.find_by(&decl.key, Lookup::Declared)
            } {
                self.merge(&path, &decl)?;
                path
            } else {
                self.insert(&decl)?
            };
        match decl.open {
            Open::Members => {
                if self.open.len() + 1 > MAX_DEPTH {
                    return Err(Declined::at(decl.line, "declarations nest too deep"));
                }
                self.members = Some((path.clone(), decl.line));
                self.slot = Slot::Attribute;
            }
            Open::Group => {
                if self.open.len() >= MAX_DEPTH {
                    return Err(Declined::at(decl.line, "packages nest too deep"));
                }
                self.node_mut(&path).composite = true;
                self.open.push((path.clone(), decl.line));
            }
            Open::None => {}
        }
        // A box declared right in `together` joins its rank.
        let depth = self.open.len() - usize::from(decl.open == Open::Group);
        if let Some((members, _, at)) = self.together.last_mut()
            && *at == depth
            && !members.iter().any(|m| m.key == decl.key)
        {
            members.push(RawName {
                key: decl.key.clone(),
                label: decl.label.clone(),
                shape: None,
                quoted: false,
            });
        }
        let home = self.here();
        for parent in &decl.parents {
            // Where names belong to their packages, a parent is made in
            // the declaration's package as it is read, as PlantUML makes
            // it; a later line finds it.
            let found = if self.near(parent.raw.shape) && parent.raw.pair().is_none() {
                let made = match self.find_near(&parent.raw.key, Lookup::Key, &home) {
                    Some(at) => at,
                    None => self.ensure(&parent.raw, &home, decl.line, true)?,
                };
                [Some(made), Some(path.clone())]
            } else {
                [None, None]
            };
            self.pending.push(Pending {
                members: [None, None],
                style: Default::default(),
                from: parent.raw.clone_name(),
                to: RawName {
                    key: decl.key.clone(),
                    label: decl.label.clone(),
                    shape: None,
                    quoted: false,
                },
                from_card: None,
                to_card: None,
                from_head: Head::Triangle,
                to_head: Head::None,
                dash: parent.dash,
                bold: false,
                hidden: false,
                toward: None,
                color: None,
                label: None,
                line: decl.line,
                home: home.clone(),
                strict: [true, false],
                found,
                note: None,
            });
        }
        let key = self.node(&path).key.clone();
        self.named.insert(key.clone());
        self.recent = Recent::Element(key, Some(path.clone()));
        self.last = Some(path);
        Ok(())
    }

    /// A name first named by a line or a member line is made then, as
    /// PlantUML makes it: a note without `of` after it is on it, and on an
    /// association's point after its pair.
    fn named_by_line(&mut self, raw: &RawName, at: Option<&Vec<usize>>) {
        if raw.pair().is_some() {
            self.recent = Recent::Point;
        } else if self.named.insert(raw.key.clone()) {
            self.recent = Recent::Element(raw.key.clone(), at.cloned());
        }
    }

    fn merge(&mut self, path: &[usize], decl: &Decl) -> Result<(), Declined> {
        let node = self.node_mut(path);
        // `class a` beside the package `a.B` made: PlantUML refuses it too.
        if !node.explicit
            && node.composite
            && node.shape == Shape::Package
            && decl.shape != Shape::Package
        {
            return Err(Declined::at(
                decl.line,
                format!("`{}` is a package already", decl.key),
            ));
        }
        if node.explicit && node.shape != decl.shape && !compatible(node.shape, decl.shape) {
            return Err(Declined::at(
                decl.line,
                format!("`{}` is already a {}", decl.key, shape_word(node.shape)),
            ));
        }
        // `()` is a component diagram's: the interface is its circle.
        if !node.explicit || matches!(decl.shape, Shape::Abstract | Shape::Lollipop) {
            node.shape = decl.shape;
        }
        node.explicit = true;
        if decl.label != decl.key || node.label.value == node.key {
            node.label = text_at(&decl.label, decl.line);
        }
        for stereotype in &decl.stereotypes {
            node.stereotypes.push(text_at(stereotype, decl.line));
        }
        // A box declared again keeps the circle a stereotype wrote it,
        // as another kind too, until a stereotype is written again, and
        // its kind's while its kind stays, as in PlantUML.
        let wanted = decl
            .spot
            .clone()
            .or_else(|| SpotKind::of_shape(node.shape).map(Spot::of));
        let keeps = !decl.spotted
            && decl.stereotypes.is_empty()
            && node.spot.as_ref().is_some_and(|have| {
                have.own || wanted.as_ref().is_some_and(|want| have.kind == want.kind)
            });
        if !keeps {
            node.spot = wanted;
        }
        if let Some(fill) = &decl.fill {
            node.fill = Some(fill.clone());
        }
        if let Some(color) = &decl.line_color {
            node.line_color = Some(color.clone());
        }
        decl.more.lay(node);
        if decl.link.is_some() {
            node.link.clone_from(&decl.link);
        }
        Ok(())
    }

    fn insert(&mut self, decl: &Decl) -> Result<Vec<usize>, Declined> {
        let here = self.here();
        self.insert_at(decl, &here, &decl.label)
    }

    /// The packages a name written with the separator (`a.b.C`) stands
    /// in, from `home`, made where they are missing, and the name's last
    /// part; `None` where the name has no separator, or is no class's,
    /// object's or package's, which are the ones PlantUML parts.
    fn qualified(
        &mut self,
        key: &str,
        shape: Shape,
        home: &[usize],
        line: usize,
    ) -> Result<Option<(Vec<usize>, String)>, Declined> {
        let Some(sep) = self.separator.clone().filter(|_| self.classes) else {
            return Ok(None);
        };
        let parted = matches!(
            shape,
            Shape::Class
                | Shape::Abstract
                | Shape::Interface
                | Shape::Enum
                | Shape::Entity
                | Shape::Annotation
                | Shape::Object
                | Shape::Package
        );
        // `.Base` names a box at the top, `.a.B` one in the top's `a`.
        let (key, from_top) = match key.strip_prefix(sep.as_str()) {
            Some(rest) => (rest, true),
            None => (key, false),
        };
        let parts: Vec<&str> = key.split(sep.as_str()).collect();
        if !parted || parts.iter().any(|part| part.trim().is_empty()) {
            return Ok(None);
        }
        if parts.len() < 2 {
            return Ok(from_top.then(|| (Vec::new(), parts[0].to_owned())));
        }
        let (packages, last) = parts.split_at(parts.len() - 1);
        let packages: Vec<String> = packages.iter().map(|p| (*p).to_owned()).collect();
        // From the top where its first package stands there (`net.dummy.P`
        // inside `namespace net.foo`), else from where it is written.
        let at_top = from_top
            || self.root.nodes.iter().any(|node| {
                node.composite && (node.key == packages[0] || node.label.value == packages[0])
            });
        let mut path = if at_top { Vec::new() } else { home.to_vec() };
        let mut prefix = String::new();
        for part in packages {
            prefix = if prefix.is_empty() {
                part.clone()
            } else {
                format!("{prefix}{sep}{part}")
            };
            let found = self.container(&path).nodes.iter().position(|node| {
                node.composite && (node.key == prefix || node.label.value == part)
            });
            let index = match found {
                Some(index) => index,
                None => {
                    if path.len() >= MAX_DEPTH {
                        return Err(Declined::at(line, "packages nest too deep"));
                    }
                    let index = self.container(&path).nodes.len();
                    self.container_mut(&path).nodes.push(Element {
                        key: prefix.clone(),
                        label: text_at(&part, line),
                        stereotypes: Vec::new(),
                        members: Vec::new(),
                        shape: Shape::Package,
                        fill: None,
                        line_color: None,
                        thickness: None,
                        text_color: None,
                        border_dash: None,
                        explicit: false,
                        composite: true,
                        children: Group::default(),
                        link: None,
                        hidden: false,
                        spot: None,
                        line,
                    });
                    index
                }
            };
            path.push(index);
        }
        Ok(Some((path, last[0].to_owned())))
    }

    /// The box a whole name (`a.b.C`) names through the packages it
    /// stands in, by their labels from the top; none is made.
    fn through_packages(&self, name: &str) -> Option<Vec<usize>> {
        self.through_from(&[], name)
    }

    /// The box a whole name (`a.b.C`) names through the packages from the
    /// one at `home`: each part a package there by its name, the last the
    /// box.
    fn through_from(&self, home: &[usize], name: &str) -> Option<Vec<usize>> {
        let sep = self.separator.as_deref().filter(|_| self.classes)?;
        let parts: Vec<&str> = name.split(sep).collect();
        if parts.len() < 2 {
            return None;
        }
        let mut path = home.to_vec();
        for (at, part) in parts.iter().enumerate() {
            let last = at + 1 == parts.len();
            let index = self.container(&path).nodes.iter().position(|node| {
                (last || node.composite) && (node.label.value == *part || node.key == *part)
            })?;
            path.push(index);
        }
        Some(path)
    }

    /// The element of the package at `home` a qualified name names: by
    /// that name, or by its last part.
    fn child(&self, home: &[usize], key: &str, label: &str) -> Option<Vec<usize>> {
        let index = self
            .container(home)
            .nodes
            .iter()
            .position(|node| node.key == key || node.key == label)?;
        let mut path = home.to_vec();
        path.push(index);
        Some(path)
    }

    fn insert_at(
        &mut self,
        decl: &Decl,
        here: &[usize],
        label: &str,
    ) -> Result<Vec<usize>, Declined> {
        let here = here.to_vec();
        let stereotypes: Vec<Text> = decl
            .stereotypes
            .iter()
            .map(|s| text_at(s, decl.line))
            .collect();
        let spot = decl
            .spot
            .clone()
            .or_else(|| SpotKind::of_shape(decl.shape).map(Spot::of));
        let index = self.container(&here).nodes.len();
        self.container_mut(&here).nodes.push(Element {
            key: decl.key.clone(),
            label: text_at(label, decl.line),
            stereotypes,
            members: Vec::new(),
            shape: decl.shape,
            fill: decl.fill.clone(),
            line_color: decl.line_color.clone(),
            thickness: decl.more.thickness,
            text_color: decl.more.text.clone(),
            border_dash: decl.more.dash,
            explicit: true,
            composite: matches!(decl.open, Open::Group),
            children: Group::default(),
            link: decl.link.clone(),
            hidden: false,
            spot,
            line: decl.line,
        });
        let mut path = here;
        path.push(index);
        Ok(path)
    }

    fn add_relation(&mut self, rel: RawRel, line: usize) -> Result<(), Declined> {
        let mut rel = rel;
        // `Foo::field1`: a line to a member of `Foo`, which meets `Foo` at
        // that member's row, unless `::` parts names into packages.
        let mut ends: MemberEnds = [None, None];
        // Not where a box has the whole name (`class std::vector`), nor
        // beside an association's pair.
        let paired = rel.from.pair().is_some() || rel.to.pair().is_some();
        if self.separator.as_deref() != Some("::") && !paired {
            for (at, raw) in [&mut rel.from, &mut rel.to].into_iter().enumerate() {
                if raw.quoted || self.find_by(&raw.key, Lookup::Key).is_some() {
                    continue;
                }
                // At the last `::`, as PlantUML parts it (`Foo::a::b` is
                // `b` of `Foo::a`).
                let split = match raw.key.find("::\"") {
                    Some(at) => Some((raw.key[..at].to_owned(), &raw.key[at + 2..])),
                    None => raw.key.rsplit_once("::").map(|(a, b)| (a.to_owned(), b)),
                };
                let Some((element, member)) = split else {
                    continue;
                };
                let (quoted, wanted) = match member.strip_prefix('"') {
                    Some(inner) => (true, inner.strip_suffix('"').unwrap_or(inner).to_owned()),
                    None => (false, member.to_owned()),
                };
                if element.trim().is_empty() || wanted.trim().is_empty() {
                    return Err(Declined::at(
                        line,
                        format!("`{}` names no member", snippet(&raw.key)),
                    ));
                }
                ends[at] = Some((quoted, wanted));
                raw.key.clone_from(&element);
                raw.label = element;
            }
        }
        // A short name finds its class where the line stands, as PlantUML
        // reads it: a `C` declared later is not the line's.
        let here = self.here();
        let (found, strict) = if self.namespaced() {
            // Where names belong to their packages, an end is found, or
            // made in the line's package, as the line is read, as PlantUML
            // does: a later line finds it. A pair and a box written in
            // brackets are found once all is read.
            let mut found = [None, None];
            for (at, raw) in [&rel.from, &rel.to].into_iter().enumerate() {
                if raw.pair().is_some() || !self.near(raw.shape) {
                    continue;
                }
                found[at] = match self.find_near(&raw.key, Lookup::Line, &here) {
                    Some(path) => Some(path),
                    None => {
                        let path = self.ensure(raw, &here, line, true)?;
                        // Made by this line: a note without `of` after it
                        // is on it.
                        self.named.remove(&raw.key);
                        Some(path)
                    }
                };
            }
            let strict = [found[0].is_none(), found[1].is_none()];
            (found, strict)
        } else {
            let found = [&rel.from, &rel.to].map(|raw| {
                self.find_by(&raw.key, Lookup::Key)
                    .is_none()
                    .then(|| self.find(&raw.key))
                    .flatten()
            });
            (
                found,
                [&rel.from, &rel.to].map(|raw| self.find(&raw.key).is_none()),
            )
        };
        self.named_by_line(&rel.from, found[0].as_ref());
        self.named_by_line(&rel.to, found[1].as_ref());
        self.pending.push(Pending {
            from: rel.from,
            to: rel.to,
            members: ends,
            style: rel.arrow.style.clone(),
            from_card: rel.from_card,
            to_card: rel.to_card,
            from_head: rel.arrow.from_head,
            to_head: rel.arrow.to_head,
            dash: rel.arrow.dash,
            bold: rel.arrow.bold,
            hidden: rel.arrow.hidden,
            toward: rel.arrow.toward,
            color: rel.arrow.color,
            label: rel.label,
            line,
            home: self.here(),
            strict,
            found,
            note: None,
        });
        Ok(())
    }

    fn add_field(&mut self, name: RawName, member: Member, line: usize) -> Result<(), Declined> {
        let home = self.here();
        let path = self.ensure(&name, &home, line, false)?;
        self.named_by_line(&name, Some(&path));
        let node = self.node_mut(&path);
        if node.composite {
            return Err(Declined::at(
                line,
                "a member on a package is not supported yet",
            ));
        }
        node.members.push(member);
        self.last = Some(path);
        Ok(())
    }

    /// The box `raw` names from a line at `home`, made where there is
    /// none; `strict`, by its key alone ([`Pending::strict`]).
    fn ensure(
        &mut self,
        raw: &RawName,
        home: &[usize],
        line: usize,
        strict: bool,
    ) -> Result<Vec<usize>, Declined> {
        let shape = raw.shape.unwrap_or(Shape::Class);
        // `a.b.C` named by a line: in the packages its name gives.
        let (home, label) = match self.qualified(&raw.key, shape, home, line)? {
            Some((packages, last)) => {
                if let Some(path) = self.child(&packages, &raw.key, &last) {
                    self.upgrade(&path, raw, line)?;
                    return Ok(path);
                }
                (packages, short_label(&raw.label, &raw.key, &last))
            }
            None => {
                let lookup = if strict { Lookup::Key } else { Lookup::Line };
                if let Some(path) = self.find_from(&raw.key, lookup, home, raw.shape) {
                    self.upgrade(&path, raw, line)?;
                    return Ok(path);
                }
                (home.to_vec(), raw.label.clone())
            }
        };
        let home = home.as_slice();
        let index = self.container(home).nodes.len();
        self.container_mut(home).nodes.push(Element {
            key: raw.key.clone(),
            label: text_at(&label, line),
            stereotypes: Vec::new(),
            members: Vec::new(),
            shape,
            fill: None,
            line_color: None,
            thickness: None,
            text_color: None,
            border_dash: None,
            explicit: false,
            composite: false,
            children: Group::default(),
            link: None,
            hidden: false,
            spot: SpotKind::of_shape(shape).map(Spot::of),
            line,
        });
        let mut path = home.to_vec();
        path.push(index);
        Ok(path)
    }

    fn upgrade(&mut self, path: &[usize], raw: &RawName, line: usize) -> Result<(), Declined> {
        let Some(shape) = raw.shape else {
            return Ok(());
        };
        let node = self.node_mut(path);
        // An interface and its circle are one ([`lollipops`]).
        let circle = |s| matches!(s, Shape::Interface | Shape::Lollipop);
        if node.shape == shape || circle(node.shape) && circle(shape) {
            if shape == Shape::Lollipop {
                node.shape = shape;
            }
            return Ok(());
        }
        if node.explicit {
            return Err(Declined::at(
                line,
                format!("`{}` is already a {}", raw.key, shape_word(node.shape)),
            ));
        }
        node.shape = shape;
        node.label = text_at(&raw.label, line);
        node.spot = SpotKind::of_shape(shape).map(Spot::of);
        Ok(())
    }

    fn find(&self, name: &str) -> Option<Vec<usize>> {
        self.find_by(name, Lookup::Line)
    }

    /// Whether names belong to the packages they stand in, as in a class
    /// diagram whose names part (`a.C` and `b.C` are two classes); in a
    /// description diagram, or under `set separator none`, a name is the
    /// whole diagram's.
    fn namespaced(&self) -> bool {
        self.classes && self.separator.is_some()
    }

    /// Whether a box of `shape` (one a line or a note names, a class
    /// unless it says more) belongs to its package: a class, an interface
    /// and their kin of a diagram whose names do; anything else, an actor
    /// or a component under `allowmixing`, is the whole diagram's, as in
    /// PlantUML.
    fn near(&self, shape: Option<Shape>) -> bool {
        self.namespaced() && shape.is_none_or(|shape| SpotKind::of_shape(shape).is_some())
    }

    /// The box `name` names, written in the package at `home`
    /// ([`Self::find_near`] where it belongs to its package, [`Self::near`]).
    fn find_from(
        &self,
        name: &str,
        lookup: Lookup,
        home: &[usize],
        shape: Option<Shape>,
    ) -> Option<Vec<usize>> {
        if self.near(shape) {
            self.find_near(name, lookup, home)
        } else {
            self.find_by(name, lookup)
        }
    }

    /// The box `name` names in the package at `home`, as PlantUML finds
    /// it where names belong to their packages: there, by its key or as
    /// the whole name made there (`a.b.C` in `a.b`); else, for a line that
    /// finds what stands already ([`Lookup::Line`]), the one box of that
    /// name anywhere. A declaration and a line that came first make one
    /// in their own package.
    fn find_near(&self, name: &str, lookup: Lookup, home: &[usize]) -> Option<Vec<usize>> {
        fn every(
            group: &Group,
            path: &mut Vec<usize>,
            named: &dyn Fn(&Element) -> bool,
            found: &mut Vec<Vec<usize>>,
        ) {
            for (index, node) in group.nodes.iter().enumerate() {
                path.push(index);
                if named(node) {
                    found.push(path.clone());
                }
                every(&node.children, path, named, found);
                path.pop();
            }
        }
        let sep = self.separator.as_deref().unwrap_or(".");
        // A whole name (`a.C`): a box of no class's kind by it, as the
        // whole diagram's (`note "x" as a.N`); a class through its
        // packages, from where the line stands, else from the top.
        if name.contains(sep) {
            let mut found = Vec::new();
            fn keyed(
                group: &Group,
                path: &mut Vec<usize>,
                name: &str,
                found: &mut Vec<Vec<usize>>,
            ) {
                for (index, node) in group.nodes.iter().enumerate() {
                    path.push(index);
                    if node.key == name
                        && SpotKind::of_shape(node.shape).is_none()
                        && !node.composite
                    {
                        found.push(path.clone());
                    }
                    keyed(&node.children, path, name, found);
                    path.pop();
                }
            }
            keyed(&self.root, &mut Vec::new(), name, &mut found);
            if let [path] = found.as_slice() {
                return Some(path.clone());
            }
            // From the top where its first package stands there, as
            // PlantUML finds it, else from where it is written.
            let first = name.split(sep).next().unwrap_or(name);
            let at_top = self
                .root
                .nodes
                .iter()
                .any(|node| node.composite && node.key == first);
            return if at_top {
                self.through_from(&[], name)
            } else {
                self.through_from(home, name)
                    .or_else(|| self.through_from(&[], name))
            };
        }
        let whole = format!("{sep}{name}");
        // By its name, or as the whole name made (`a.C` of `C`); an
        // alias's words (`"User" as U1`) name no box.
        let named = |node: &Element| {
            node.key == name || (node.label.value == name && node.key.ends_with(&whole))
        };
        let here = named;
        if let Some(at) = self.container(home).nodes.iter().position(here) {
            return Some([home, &[at]].concat());
        }
        if lookup != Lookup::Line {
            return None;
        }
        let mut found = Vec::new();
        every(&self.root, &mut Vec::new(), &named, &mut found);
        match found.len() {
            1 => found.pop(),
            _ => None,
        }
    }

    /// The box `name` names: by its key; else, unless `lookup` takes keys
    /// alone, by its label where one box alone has it, a class made from
    /// a whole name (`net.Socket`) not for a declaration
    /// ([`Lookup::Declared`]), as PlantUML finds them.
    fn find_by(&self, name: &str, lookup: Lookup) -> Option<Vec<usize>> {
        if let Some(path) = self.find_in(&self.root, &[], name, true) {
            return Some(path);
        }
        if lookup == Lookup::Key {
            return None;
        }
        let mut found = Vec::new();
        self.labeled(&self.root, &mut Vec::new(), name, lookup, &mut found);
        match found.len() {
            1 => found.pop(),
            _ => None,
        }
    }

    fn labeled(
        &self,
        group: &Group,
        path: &mut Vec<usize>,
        name: &str,
        lookup: Lookup,
        found: &mut Vec<Vec<usize>>,
    ) {
        for (index, node) in group.nodes.iter().enumerate() {
            path.push(index);
            let made = self.separator.as_ref().is_some_and(|sep| {
                node.key != node.label.value
                    && node.key.ends_with(&format!("{sep}{}", node.label.value))
            });
            if node.label.value == name && node.key != name && !(made && lookup == Lookup::Declared)
            {
                found.push(path.clone());
            }
            self.labeled(&node.children, path, name, lookup, found);
            path.pop();
        }
    }

    fn find_in(
        &self,
        group: &Group,
        path: &[usize],
        name: &str,
        by_key: bool,
    ) -> Option<Vec<usize>> {
        for (index, node) in group.nodes.iter().enumerate() {
            let hit = if by_key {
                node.key == name
            } else {
                node.label.value == name && node.key != name
            };
            let mut next = path.to_vec();
            next.push(index);
            if hit {
                return Some(next);
            }
            if let Some(found) = self.find_in(&node.children, &next, name, by_key) {
                return Some(found);
            }
        }
        None
    }

    fn place_link(
        &mut self,
        from: Vec<usize>,
        to: Vec<usize>,
        item: &Pending,
    ) -> Result<(), Declined> {
        if from.is_empty() || to.is_empty() {
            return Err(Declined::at(item.line, "a line needs two elements"));
        }
        // Each end's member on its box, by its place there; `hide` keeps
        // the place to the members it leaves.
        let place = |path: &[usize], end: &Option<(bool, String)>| {
            let (quoted, name) = end.as_ref()?;
            self.node(path)
                .members
                .iter()
                .position(|m| member_is(m, *quoted, name))
        };
        let members = [place(&from, &item.members[0]), place(&to, &item.members[1])];
        let link = |from: usize, to: usize| Link {
            from,
            to,
            label: item.label.as_ref().map(|text| text_at(text, item.line)),
            from_card: item.from_card.clone(),
            to_card: item.to_card.clone(),
            from_head: item.from_head,
            to_head: item.to_head,
            color: item.color.clone(),
            dash: item.dash,
            bold: item.bold,
            hidden: item.hidden,
            level: None,
            line: item.line,
            label_before: None,
            note: item.note.clone(),
            members,
            style: item.style.clone(),
        };
        if from == to {
            let index = *from.last().expect("a path");
            let home = from[..from.len() - 1].to_vec();
            self.container_mut(&home).edges.push(link(index, index));
            return Ok(());
        }
        // A direction along the flow ranks the two, against it from the
        // second box to the first, heads and multiplicities with it;
        // across it the second box stands in the rank of the first, on
        // that side: held there by a hidden line (docs/adr/0021), joined
        // there by one that is drawn (docs/adr/0027).
        // The words are said as if the flow ran down, and turn with it:
        // where it runs to the right, `-down->` runs along it, as the Java
        // PlantUML lays it.
        let along = match item.toward {
            None => true,
            Some(ACROSS | BESIDE) => false,
            Some(word) => matches!(word, "down" | "up"),
        };
        let forward = item
            .toward
            .is_none_or(|word| matches!(word, "down" | "right" | ACROSS | BESIDE));
        let turned = |from: usize, to: usize| Link {
            from_head: item.to_head,
            to_head: item.from_head,
            from_card: item.to_card.clone(),
            to_card: item.from_card.clone(),
            members: [members[1], members[0]],
            ..link(to, from)
        };
        let home = &from[..from.len() - 1];
        if to.len() == from.len() && to.starts_with(home) {
            let from_i = *from.last().expect("a path");
            let to_i = *to.last().expect("a path");
            let group = self.container_mut(home);
            match (along, forward) {
                (true, true) => group.edges.push(link(from_i, to_i)),
                (true, false) => group.edges.push(turned(from_i, to_i)),
                (false, after) if item.hidden => group.held.push(crate::graph::hold::Held {
                    line: item.line,
                    index: to_i,
                    beside: from_i,
                    word: crate::graph::hold::Word::beside(after),
                }),
                (false, after) => group.edges.push(Link {
                    level: Some(after),
                    ..link(from_i, to_i)
                }),
            }
            return Ok(());
        }
        if !along && item.toward != Some(BESIDE) {
            // `together` holds the boxes of one package alone; across
            // packages they stand where the layout puts them.
            if item.hidden && item.toward == Some(ACROSS) {
                return Ok(());
            }
            return Err(Declined::at(
                item.line,
                "a line with a direction across the flow between boxes of different packages is not supported yet",
            ));
        }
        let Some((depth, a, b)) = meeting(&from, &to) else {
            return Err(Declined::at(
                item.line,
                "a line between a box and something inside it is not supported yet",
            ));
        };
        let crossing = if forward {
            Crossing {
                from: from[depth..].to_vec(),
                to: to[depth..].to_vec(),
                arrow: link(a, b),
            }
        } else {
            Crossing {
                from: to[depth..].to_vec(),
                to: from[depth..].to_vec(),
                arrow: turned(a, b),
            }
        };
        self.container_mut(&from[..depth]).crossings.push(crossing);
        Ok(())
    }

    fn place_notes(&mut self) -> Result<(), Declined> {
        for note in std::mem::take(&mut self.raw_notes) {
            let found = match &note.at {
                Some(at) => Some(at.clone()),
                None => self.find_from(&note.target, Lookup::Line, &note.home, None),
            };
            let (path, member) = match found {
                Some(path) => (path, None),
                None => match self.through_packages(&note.target) {
                    Some(path) => (path, None),
                    None => self.member_of(&note.target, &note.home, note.line)?,
                },
            };
            let index = *path.last().expect("a path");
            let home = path[..path.len() - 1].to_vec();
            if let Some((quoted, wanted)) = member {
                self.on_members.push((note.line, quoted, wanted));
            }
            self.container_mut(&home).notes.push(Note {
                target: index,
                member: None,
                side: note.side,
                text: note.text,
                line: note.line,
                fill: note.fill,
                line_color: note.line_color,
            });
        }
        Ok(())
    }

    /// Each note on a member, on the member's place among those `hide`
    /// left; one whose member is hidden is on its element.
    fn resolve_members(&mut self) {
        fn walk(group: &mut Group, wanted: &[(usize, bool, String)]) {
            for note in &mut group.notes {
                let Some((_, quoted, name)) = wanted.iter().find(|(line, ..)| *line == note.line)
                else {
                    continue;
                };
                note.member = group.nodes.get(note.target).and_then(|node| {
                    node.members
                        .iter()
                        .position(|m| member_is(m, *quoted, name))
                });
            }
            for node in &mut group.nodes {
                walk(&mut node.children, wanted);
            }
        }
        let wanted = std::mem::take(&mut self.on_members);
        if !wanted.is_empty() {
            walk(&mut self.root, &wanted);
        }
    }

    /// The element and the member a note's `A::count` or
    /// `A::"start(int ms)"` names, as PlantUML finds it: a member by its
    /// name, or by its words where they are quoted; whether they are, and
    /// the name or the words.
    #[allow(clippy::type_complexity)]
    fn member_of(
        &self,
        target: &str,
        home: &[usize],
        line: usize,
    ) -> Result<(Vec<usize>, Option<(bool, String)>), Declined> {
        let missing = || Declined::at(line, format!("`{target}` is not an element"));
        let (element, member) = match target.find("::\"") {
            Some(at) => (&target[..at], &target[at + 2..]),
            None => target.rsplit_once("::").ok_or_else(missing)?,
        };
        let path = self
            .find_from(element.trim(), Lookup::Line, home, None)
            .ok_or_else(missing)?;
        let (quoted, wanted) = match member.trim().strip_prefix('"') {
            Some(inner) => (true, inner.strip_suffix('"').unwrap_or(inner)),
            None => (false, member.trim()),
        };
        let index = self
            .node(&path)
            .members
            .iter()
            .position(|m| member_is(m, quoted, wanted));
        match index {
            Some(_) => Ok((path, Some((quoted, wanted.to_owned())))),
            None => Err(Declined::at(
                line,
                format!("`{}` has no member `{wanted}`", element.trim()),
            )),
        }
    }

    fn open_last(&mut self, line: usize) -> Result<(), Declined> {
        let Some(path) = self.last.clone() else {
            return Err(Declined::at(line, "there is no declaration for this `{`"));
        };
        let shape = self.node(&path).shape;
        match open_of(shape) {
            Open::None => Err(Declined::at(
                line,
                format!("a {} has no body", shape_word(shape)),
            )),
            Open::Members => {
                self.members = Some((path, line));
                self.slot = Slot::Attribute;
                Ok(())
            }
            Open::Group => {
                if self.open.len() >= MAX_DEPTH {
                    return Err(Declined::at(line, "packages nest too deep"));
                }
                self.node_mut(&path).composite = true;
                self.open.push((path, line));
                Ok(())
            }
        }
    }

    fn close(&mut self, line: usize) -> Result<(), Declined> {
        if self.open.pop().is_none() {
            return Err(Declined::at(line, "there is no `{` for this `}`"));
        }
        Ok(())
    }

    /// `note on link : words`, `note right on link #pink` and its words up
    /// to `end note`: a note on the line written last, as PlantUML reads
    /// it; `None` when the note is another.
    fn link_note(
        &mut self,
        lines: &[Line],
        at: usize,
        rest: &str,
    ) -> Result<Option<usize>, Declined> {
        let Line { no, text, .. } = lines[at];
        let mut words = rest.split_whitespace();
        let first = words.next().unwrap_or_default().to_ascii_lowercase();
        let (side, after) = match first.as_str() {
            "left" => (Some(Side::Left), keyword(rest, "left")),
            "right" => (Some(Side::Right), keyword(rest, "right")),
            "top" => (Some(Side::Top), keyword(rest, "top")),
            "bottom" => (Some(Side::Bottom), keyword(rest, "bottom")),
            _ => (None, Some(rest)),
        };
        // `link`, or `link:` with its words right after it.
        let Some(after) = after
            .and_then(|after| keyword(after, "on"))
            .and_then(|after| {
                keyword(after, "link").or_else(|| {
                    let (word, rest) = after.split_at_checked(4)?;
                    (word.eq_ignore_ascii_case("link") && rest.starts_with(':')).then_some(rest)
                })
            })
        else {
            return Ok(None);
        };
        let mut after = after.trim_start();
        let mut fill = None;
        if after.starts_with('#') {
            let end = after
                .find(|c: char| c.is_whitespace() || c == ':')
                .unwrap_or(after.len());
            fill = Some(after[..end].to_owned());
            after = after[end..].trim_start();
        }
        let (note, next) = if let Some(words) = after.strip_prefix(':') {
            let words = words.trim();
            let col = lines[at].at + offset_in(text, words);
            let note = Text {
                value: words.to_owned(),
                line: no,
                col: Some(col),
            };
            (note, at + 1)
        } else if after.is_empty() {
            let (body, next) = block_text(lines, at, "note", end_note)?;
            (body, next)
        } else {
            return Err(Declined::at(
                no,
                format!("`note on link {}` is not supported yet", snippet(after)),
            ));
        };
        if note.value.trim().is_empty() {
            return Err(Declined::at(no, "a note needs text"));
        }
        // The line written last, as PlantUML takes it: not one to a note,
        // nor one `together` holds its boxes by.
        let notes: Vec<bool> = self
            .pending
            .iter()
            .map(|item| {
                [&item.from, &item.to].iter().any(|end| {
                    self.find_from(&end.key, Lookup::Line, &item.home, end.shape)
                        .is_some_and(|path| self.node(&path).shape == Shape::Note)
                })
            })
            .collect();
        let Some(line) = self
            .pending
            .iter_mut()
            .zip(notes)
            .rev()
            .find(|(item, to_note)| !to_note && !(item.hidden && item.toward == Some(ACROSS)))
            .map(|(item, _)| item)
        else {
            return Err(Declined::at(no, "`note on link` needs a line before it"));
        };
        if line.hidden {
            return Err(Declined::at(no, "a hidden line takes no note"));
        }
        // A second note on it takes the first one's place, as in PlantUML.
        line.note = Some(super::LinkNote {
            side,
            text: note,
            fill,
        });
        // A note without `of` after it is on the box made last still.
        Ok(Some(next))
    }

    fn note(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line {
            no,
            text,
            at: start,
            ..
        } = lines[at];
        if keyword(text, "hnote").is_some() {
            return Err(Declined::at(no, "`hnote` is not supported yet"));
        }
        let rest = keyword(text, "rnote")
            .or_else(|| keyword(text, "note"))
            .unwrap_or(text);
        if let Some(next) = self.link_note(lines, at, rest)? {
            return Ok(next);
        }
        if keyword(rest, "over").is_some() {
            return Err(Declined::at(
                no,
                "a note over several elements is not supported yet",
            ));
        }
        if rest.starts_with('"') {
            let (body, after) = take_quoted(rest, no)?;
            let key = note_alias(after, no, &mut self.notes_made)?;
            // Its words stand in the quotes, before its alias.
            let label = Text {
                value: body,
                line: no,
                col: Some(start + offset_in(text, rest) + 1),
            };
            self.insert_note(&key, label, no)?;
            return Ok(at + 1);
        }
        if let Some(alias) = keyword(rest, "as") {
            let key = alias.trim();
            if key.is_empty() {
                return Err(Declined::at(no, "`as` needs a name"));
            }
            let key = key.to_owned();
            let (label, next) = block_text(lines, at, "note", end_note)?;
            self.insert_note(&key, label, no)?;
            return Ok(next);
        }
        if let Some((side, after)) = side_of(rest) {
            // Without `of`, on what was made last (`note left: …`, or a
            // block), as PlantUML reads it: an element, or a note.
            let words = after.trim_start();
            if keyword(after, "of").is_none() && (words.is_empty() || words.starts_with(':')) {
                let (target, on) = match &self.recent {
                    Recent::Element(key, on) => (key.clone(), on.clone()),
                    Recent::Note => {
                        return Err(Declined::at(no, "a note on a note is not supported yet"));
                    }
                    Recent::Point => {
                        return Err(Declined::at(
                            no,
                            "a note on an association's point is not supported yet",
                        ));
                    }
                    Recent::Nothing => {
                        return Err(Declined::at(no, "a note needs an element before it"));
                    }
                };
                let (body, next) = match words.strip_prefix(':') {
                    Some(text) => (text_at(text.trim(), no), at + 1),
                    None => block_text(lines, at, "note", end_note)?,
                };
                self.recent = Recent::Note;
                self.raw_notes.push(RawNote {
                    target,
                    home: self.here(),
                    at: on,
                    side,
                    text: body,
                    line: no,
                    fill: None,
                    line_color: None,
                });
                return Ok(next);
            }
            let Some(after) = keyword(after, "of") else {
                return Err(Declined::at(no, "a note needs `of` and an element"));
            };
            let (name, colors, inline) = note_of(after, no)?;
            let (_, fill, line_color, more) = take_colors(colors, no)?;
            if more != More::default() {
                return Err(Declined::at(
                    no,
                    "a style on a note beyond its colors is not supported yet",
                ));
            }
            let name = name.trim().trim_matches('"');
            if name.is_empty() {
                return Err(Declined::at(no, "a note needs an element"));
            }
            // PlantUML takes a note on a member as a block only.
            if inline.is_some() && name.contains("::") && self.find(name).is_none() {
                return Err(Declined::refused(
                    no,
                    format!(
                        "PlantUML takes a note on a member (`{name}`) as a block only, with `end note`"
                    ),
                ));
            }
            let (body, next) = if let Some(text) = inline {
                (text_at(text, no), at + 1)
            } else {
                block_text(lines, at, "note", end_note)?
            };
            self.recent = Recent::Note;
            self.raw_notes.push(RawNote {
                target: name.to_owned(),
                home: self.here(),
                at: None,
                side,
                text: body,
                line: no,
                fill,
                line_color,
            });
            return Ok(next);
        }
        Err(Declined::at(
            no,
            format!("`{}` is not supported yet", snippet(text)),
        ))
    }

    fn insert_note(&mut self, key: &str, label: Text, line: usize) -> Result<(), Declined> {
        // A box of that name stands already where the note is written:
        // PlantUML refuses it. One of another package is another, where
        // names belong to their packages; else names are the diagram's.
        let here = self.here();
        let taken = if self.namespaced() {
            self.container(&here)
                .nodes
                .iter()
                .any(|node| node.key == key)
        } else {
            self.find_by(key, Lookup::Key).is_some()
        };
        if taken {
            return Err(Declined::refused(
                line,
                format!("`{key}` is an element already, and no note"),
            ));
        }
        self.recent = Recent::Note;
        let here = self.here();
        let index = self.container(&here).nodes.len();
        self.container_mut(&here).nodes.push(Element {
            key: key.to_owned(),
            label,
            stereotypes: Vec::new(),
            members: Vec::new(),
            shape: Shape::Note,
            fill: None,
            line_color: None,
            thickness: None,
            text_color: None,
            border_dash: None,
            explicit: true,
            composite: false,
            children: Group::default(),
            link: None,
            hidden: false,
            spot: None,
            line,
        });
        let mut path = here;
        path.push(index);
        self.last = Some(path);
        Ok(())
    }
}

impl RawName {
    fn clone_name(&self) -> Self {
        Self {
            key: self.key.clone(),
            label: self.label.clone(),
            shape: self.shape,
            quoted: self.quoted,
        }
    }
}

/// Whether `member` is the one a note names: by its quoted words, or by
/// its [`member_name`].
fn member_is(member: &Member, quoted: bool, wanted: &str) -> bool {
    match member {
        Member::Text { text, .. } if quoted => text.value.contains(wanted),
        Member::Text { text, .. } => member_name(&text.value) == wanted,
        _ => false,
    }
}

/// The name of a member as a note names it: a method's before its `(`,
/// an attribute's before its `:` type (one colon, not a `::` of its type)
/// or after its own type (`int counter`, `std::string name`), its value
/// (`= 0`) and its visibility left out.
fn member_name(text: &str) -> &str {
    let head = text.split('(').next().unwrap_or(text);
    let head = head.split('=').next().unwrap_or(head);
    let single = head
        .match_indices(':')
        .map(|(at, _)| at)
        .find(|&at| !head[at + 1..].starts_with(':') && !head[..at].ends_with(':'));
    let head = match single {
        Some(at) if !text.contains('(') => &head[..at],
        _ => head,
    };
    head.split_whitespace()
        .last()
        .unwrap_or("")
        .trim_start_matches(['+', '-', '#', '~'])
}

fn end_note(text: &str) -> bool {
    squeezed_eq(text, "endnote") || squeezed_eq(text, "endrnote")
}

fn note_alias(after: &str, line: usize, made: &mut usize) -> Result<String, Declined> {
    let after = after.trim();
    if after.is_empty() {
        *made += 1;
        return Ok(format!("\u{1}note{made}"));
    }
    let Some(alias) = keyword(after, "as") else {
        return Err(Declined::at(
            line,
            format!("`{}` is not supported yet", snippet(after)),
        ));
    };
    if alias.is_empty() {
        return Err(Declined::at(line, "`as` needs a name"));
    }
    Ok(alias.to_owned())
}

fn side_of(text: &str) -> Option<(Side, &str)> {
    for (word, side) in [
        ("left", Side::Left),
        ("right", Side::Right),
        ("top", Side::Top),
        ("bottom", Side::Bottom),
    ] {
        if let Some(rest) = keyword(text, word) {
            return Some((side, rest));
        }
        // `note left: words`, the colon right after the side.
        if text
            .get(..word.len())
            .is_some_and(|w| w.eq_ignore_ascii_case(word))
            && text[word.len()..].starts_with(':')
        {
            return Some((side, &text[word.len()..]));
        }
    }
    None
}

fn rejected(text: &str) -> Option<&'static str> {
    [
        "together",
        "sprite",
        "skin",
        "style",
        "url",
        "link",
        "map",
        "json",
        "port",
        "portin",
        "portout",
        "circle",
        "diamond",
        "boundary",
        "control",
        "collections",
    ]
    .into_iter()
    .find(|word| keyword(text, word).is_some() && !names_an_end(text, word))
}

/// Whether `text` starts with a line from something called `word`:
/// `Node --> Node : parent` is a line from a class called Node, not a
/// node called `-->`.
fn names_an_end(text: &str, word: &str) -> bool {
    find_arrow(text).is_some_and(|(arrow, _)| text[..arrow].trim().len() == word.len())
}

fn leading_shape(text: &str) -> Option<(Shape, Open, &str)> {
    const PHRASES: &[(&str, Shape, Open)] = &[
        ("abstract class", Shape::Abstract, Open::Members),
        ("abstract", Shape::Abstract, Open::Members),
        ("annotation", Shape::Annotation, Open::Members),
        ("interface", Shape::Interface, Open::Members),
        ("dataclass", Shape::Class, Open::Members),
        ("exception", Shape::Class, Open::Members),
        ("metaclass", Shape::Class, Open::Members),
        ("protocol", Shape::Interface, Open::Members),
        ("component", Shape::Component, Open::Group),
        ("namespace", Shape::Package, Open::Group),
        ("rectangle", Shape::Rectangle, Open::Group),
        ("artifact", Shape::Artifact, Open::Group),
        ("database", Shape::Database, Open::Group),
        ("usecase", Shape::UseCase, Open::None),
        ("storage", Shape::Storage, Open::Group),
        ("hexagon", Shape::Hexagon, Open::Group),
        ("package", Shape::Package, Open::Group),
        ("folder", Shape::Folder, Open::Group),
        ("object", Shape::Object, Open::Members),
        ("struct", Shape::Class, Open::Members),
        ("record", Shape::Class, Open::Members),
        ("class", Shape::Class, Open::Members),
        ("cloud", Shape::Cloud, Open::Group),
        ("frame", Shape::Frame, Open::Group),
        ("person", Shape::Person, Open::None),
        ("queue", Shape::Queue, Open::None),
        ("actor", Shape::Actor, Open::None),
        ("agent", Shape::Agent, Open::Group),
        ("stack", Shape::Stack, Open::Group),
        ("node", Shape::Node, Open::Group),
        ("entity", Shape::Entity, Open::Members),
        ("enum", Shape::Enum, Open::Members),
        ("card", Shape::Card, Open::Group),
        ("file", Shape::File, Open::Group),
        ("()", Shape::Lollipop, Open::None),
    ];
    for &(word, shape, open) in PHRASES {
        if let Some(rest) = phrase(text, word) {
            // `File : name` is a member of a class called File; `actor
            // :User:` an actor.
            let member = rest
                .strip_prefix(':')
                .is_some_and(|after| after.is_empty() || after.starts_with(char::is_whitespace));
            return (!names_an_end(text, word) && !member).then_some((shape, open, rest));
        }
    }
    None
}

/// Whether a line inside a body declares something of its own, most
/// likely after a missing `}`: `class Inner {` or `note left of A`.
/// Members such as `FILE`, `node : Node` or `abstract void draw()` do not.
fn opens_declaration(text: &str) -> bool {
    if let Some(rest) = keyword(text, "note") {
        return side_of(rest).is_some_and(|(_, after)| keyword(after, "of").is_some());
    }
    text.ends_with('{')
        && leading_shape(text)
            .is_some_and(|(_, _, rest)| rest.starts_with(|c: char| c.is_alphanumeric() || c == '"'))
}

fn phrase<'a>(text: &'a str, word: &str) -> Option<&'a str> {
    if !starts_with_ci(text, word) {
        return None;
    }
    let rest = &text[word.len()..];
    match rest.chars().next() {
        None => Some(rest),
        Some(c)
            if c.is_whitespace() || matches!(c, '{' | '<' | '"' | '(' | '[' | ':' | '#' | '.') =>
        {
            Some(rest.trim())
        }
        _ => None,
    }
}

/// The interfaces of a component diagram as PlantUML draws them there: a
/// circle, its name under it. A diagram with a component, a node, an actor
/// or another element only a component, deployment or use case diagram
/// has is one; an interface with members belongs to a class diagram.
fn lollipops(root: &mut Group) {
    fn described(group: &Group) -> bool {
        group.nodes.iter().any(|node| {
            matches!(
                node.shape,
                Shape::Lollipop
                    | Shape::Component
                    | Shape::UseCase
                    | Shape::Actor
                    | Shape::Node
                    | Shape::Artifact
                    | Shape::Cloud
                    | Shape::Database
                    | Shape::Folder
                    | Shape::File
                    | Shape::Card
                    | Shape::Storage
                    | Shape::Stack
                    | Shape::Hexagon
                    | Shape::Agent
                    | Shape::Queue
                    | Shape::Person
            ) || described(&node.children)
        })
    }
    fn circles(group: &mut Group) {
        for node in &mut group.nodes {
            if matches!(node.shape, Shape::Interface | Shape::Lollipop)
                && node.members.is_empty()
                && !node.composite
            {
                node.shape = Shape::Lollipop;
                node.stereotypes.retain(|s| s.value != "interface");
            }
            circles(&mut node.children);
        }
    }
    if described(root) {
        circles(root);
    } else {
        // Without components, an interface is a class diagram's.
        fn boxes(group: &mut Group) {
            for node in &mut group.nodes {
                if node.shape == Shape::Lollipop {
                    node.shape = Shape::Interface;
                }
                boxes(&mut node.children);
            }
        }
        boxes(root);
    }
}

fn open_of(shape: Shape) -> Open {
    match shape {
        Shape::Class
        | Shape::Abstract
        | Shape::Interface
        | Shape::Enum
        | Shape::Entity
        | Shape::Annotation
        | Shape::Object => Open::Members,
        Shape::UseCase | Shape::Actor | Shape::Note | Shape::Lollipop => Open::None,
        _ => Open::Group,
    }
}

fn shape_word(shape: Shape) -> &'static str {
    match shape {
        Shape::Class | Shape::Abstract => "class",
        Shape::Interface | Shape::Lollipop => "interface",
        Shape::Enum => "enum",
        Shape::Entity => "entity",
        Shape::Annotation => "annotation",
        Shape::Component => "component",
        Shape::UseCase => "use case",
        Shape::Actor => "actor",
        Shape::Node => "node",
        Shape::Artifact => "artifact",
        Shape::Cloud => "cloud",
        Shape::Database => "database",
        Shape::Folder => "folder",
        Shape::Frame => "frame",
        Shape::Rectangle => "rectangle",
        Shape::Object => "object",
        Shape::Package => "package",
        Shape::File => "file",
        Shape::Card => "card",
        Shape::Storage => "storage",
        Shape::Stack => "stack",
        Shape::Hexagon => "hexagon",
        Shape::Agent => "agent",
        Shape::Person => "person",
        Shape::Queue => "queue",
        Shape::Note => "note",
        Shape::Point => "point",
    }
}

fn compatible(a: Shape, b: Shape) -> bool {
    matches!(
        (a, b),
        (Shape::Class, Shape::Abstract)
            | (Shape::Abstract, Shape::Class)
            | (Shape::Interface, Shape::Lollipop)
            | (Shape::Lollipop, Shape::Interface)
    )
}

fn declaration(text: &str, line: usize) -> Result<Option<Decl>, Declined> {
    if let Some(rest) = keyword(text, "archimate") {
        return archimate(rest, line).map(Some);
    }
    let Some((shape, open, rest)) = leading_shape(text) else {
        return Ok(None);
    };
    // `() "Clipboard" --> [Renderer]` is a line, its end an interface.
    if shape == Shape::Lollipop && find_arrow(text).is_some() {
        return Ok(None);
    }
    let mut decl = parse_decl(shape, open, rest, line)?;
    if let Some(kind) = spot_kind(text, shape) {
        let spot = decl.spot.get_or_insert_with(|| Spot::of(kind));
        spot.kind = kind;
    }
    Ok(Some(decl))
}

/// What a class's keyword makes it: `struct`, `record` and their kin are
/// classes with a circle of their own.
fn spot_kind(text: &str, shape: Shape) -> Option<SpotKind> {
    const WORDS: [(&str, SpotKind); 6] = [
        ("dataclass", SpotKind::DataClass),
        ("exception", SpotKind::Exception),
        ("metaclass", SpotKind::MetaClass),
        ("protocol", SpotKind::Protocol),
        ("struct", SpotKind::Struct),
        ("record", SpotKind::Record),
    ];
    WORDS
        .iter()
        .find(|(word, _)| phrase(text, word).is_some())
        .map(|&(_, kind)| kind)
        .or_else(|| SpotKind::of_shape(shape))
}

/// The spot `<< (S,#FF7700) >>` writes on a box of `shape`, of its
/// shape's kind until a keyword says more.
fn own_spot(
    own: Option<(char, Option<String>)>,
    shape: Shape,
    line: usize,
) -> Result<Option<Spot>, Declined> {
    let Some((letter, color)) = own else {
        return Ok(None);
    };
    let Some(kind) = SpotKind::of_shape(shape) else {
        return Err(Declined::at(
            line,
            format!("a {} has no circle before its name", shape_word(shape)),
        ));
    };
    Ok(Some(Spot {
        kind,
        letter,
        color,
        own: true,
    }))
}

/// `archimate #Business "Customer" as c <<business-actor>>`: a box in the
/// color of its layer, the icon of its type over its name, as PlantUML
/// draws it; the stereotype that names the type is not shown.
fn archimate(rest: &str, line: usize) -> Result<Decl, Declined> {
    let mut decl = parse_decl(Shape::Rectangle, Open::Group, rest, line)?;
    if let Some(at) = decl
        .stereotypes
        .iter()
        .position(|s| crate::icons::archimate::icon(s).is_some())
    {
        let kind = decl.stereotypes.remove(at);
        decl.label = format!("<$archimate/{}>\\n{}", kind.trim(), decl.label);
    }
    if let Some(fill) = &decl.fill
        && let Some(layer) = layer_color(fill)
    {
        decl.fill = Some(layer.to_owned());
    }
    Ok(decl)
}

/// The color PlantUML gives an ArchiMate layer by its name (`#Business`).
pub(crate) fn layer_color(name: &str) -> Option<&'static str> {
    Some(
        match name.trim_start_matches('#').to_ascii_lowercase().as_str() {
            "business" => "#FFFFCC",
            "application" => "#C2F0FF",
            "technology" => "#C9FFC9",
            "motivation" => "#CCCCFF",
            "strategy" => "#F8E7C0",
            "implementation" => "#FFE0E0",
            "physical" => "#97FF97",
            _ => return None,
        },
    )
}

fn parse_decl(shape: Shape, open: Open, rest: &str, line: usize) -> Result<Decl, Declined> {
    let mut rest = rest.trim();
    let mut opened = false;
    if let Some(body) = rest.strip_suffix('{') {
        opened = true;
        rest = body.trim();
    }
    let (rest, link) = take_link(rest, line)?;
    if rest.is_empty() {
        return Err(Declined::at(line, "a declaration needs a name"));
    }
    let (rest, stereotypes, own) = take_stereotypes(&rest, line)?;
    let spotted = own.is_some();
    let spot = own_spot(own, shape, line)?;
    let (rest, fill, line_color, more) = take_colors(&rest, line)?;
    let (name, parents) = take_inherits(rest.trim(), line)?;
    if name.is_empty() {
        return Err(Declined::at(line, "a declaration needs a name"));
    }
    // `class Crate<T>` is the class Crate, its type parameters shown.
    let (name, generic) = take_generic(&name);
    let (key, mut label, _) = split_alias(name, line)?;
    if let Some(generic) = generic {
        label = format!("{label}<{generic}>");
    }
    if key.is_empty() {
        return Err(Declined::at(line, "a declaration needs a name"));
    }
    if opened && matches!(open, Open::None) {
        return Err(Declined::at(
            line,
            format!("a {} has no body", shape_word(shape)),
        ));
    }
    Ok(Decl {
        key,
        label,
        shape,
        stereotypes,
        spot,
        spotted,
        fill,
        line_color,
        more,
        open: if opened { open } else { Open::None },
        parents,
        link,
        line,
    })
}

fn bare(text: &str, line: usize) -> Result<Option<Decl>, Declined> {
    if find_arrow(text).is_some() {
        return Ok(None);
    }
    let mut rest = text.trim();
    let mut opened = false;
    if let Some(body) = rest.strip_suffix('{') {
        opened = true;
        rest = body.trim();
    }
    let (rest, link) = take_link(rest, line)?;
    let (rest, stereotypes, own) = take_stereotypes(&rest, line)?;
    let (rest, fill, line_color, more) = take_colors(&rest, line)?;
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(None);
    }
    let shaped = rest.starts_with(['(', '[', ':', '"']);
    if !shaped
        && split_as(rest).is_none()
        && stereotypes.is_empty()
        && own.is_none()
        && fill.is_none()
    {
        return Ok(None);
    }
    let (key, label, shape) = split_alias(rest, line)?;
    let shape = shape.unwrap_or(Shape::Class);
    let spotted = own.is_some();
    let spot = own_spot(own, shape, line)?;
    let open = open_of(shape);
    if opened && matches!(open, Open::None) {
        return Err(Declined::at(
            line,
            format!("a {} has no body", shape_word(shape)),
        ));
    }
    Ok(Some(Decl {
        key,
        label,
        shape,
        stereotypes,
        spot,
        spotted,
        fill,
        line_color,
        more,
        open: if opened { open } else { Open::None },
        parents: Vec::new(),
        link,
        line,
    }))
}

/// What is left of `input`, its stereotypes, and the spot one of them
/// writes (`<< (S,#FF7700) Singleton >>`).
type Stereotyped = (String, Vec<String>, Option<(char, Option<String>)>);

fn take_stereotypes(input: &str, line: usize) -> Result<Stereotyped, Declined> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut stereos = Vec::new();
    let mut spot = None;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '<' && chars.get(i + 1) == Some(&'<') {
            let start = i + 2;
            let mut j = start;
            let mut end = None;
            while j + 1 < chars.len() {
                if chars[j] == '>' && chars[j + 1] == '>' {
                    end = Some(j);
                    break;
                }
                j += 1;
            }
            let Some(end) = end else {
                return Err(Declined::at(line, "a stereotype has no `>>`"));
            };
            let text: String = chars[start..end].iter().collect();
            let (own, rest) = crate::common::spotted(&text);
            let spotted = own.is_some();
            if spotted {
                if spot.is_some() {
                    return Err(Declined::at(line, "a box has one circle before its name"));
                }
                spot = own;
            }
            // What is no spot is words, as PlantUML shows them
            // (`<<x, y>>`, `<<#red>>`).
            let text = rest.unwrap_or_default();
            if text.is_empty() && !spotted {
                return Err(Declined::at(line, "a stereotype needs a text"));
            }
            if !text.is_empty() {
                stereos.push(text);
            }
            i = end + 2;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok((out, stereos, spot))
}

/// What an inline style says beyond the fill and the line's color: the
/// color of the words, a dashed or dotted border, a bold one.
#[derive(Debug, Clone, Default, PartialEq)]
struct More {
    text: Option<String>,
    dash: Option<(f32, f32)>,
    thickness: Option<f32>,
}

impl More {
    /// Onto a box declared before: what is said here replaces it.
    fn lay(&self, node: &mut Element) {
        if self.text.is_some() {
            node.text_color.clone_from(&self.text);
        }
        if self.dash.is_some() {
            node.border_dash = self.dash;
        }
        if self.thickness.is_some() {
            node.thickness = self.thickness;
        }
    }
}

type Colors = (String, Option<String>, Option<String>, More);

fn take_colors(input: &str, line: usize) -> Result<Colors, Declined> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::new();
    let mut fill = None;
    let mut line_color = None;
    let mut more = More::default();
    let mut i = 0;
    let mut quote = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            quote = !quote;
            out.push(c);
            i += 1;
            continue;
        }
        if !quote && c == '#' {
            let start = i;
            i += 1;
            while i < chars.len()
                && !chars[i].is_whitespace()
                && !matches!(chars[i], '{' | '<' | '"' | ',')
            {
                i += 1;
            }
            let token: String = chars[start..i].iter().collect();
            // `##[dashed]green`: a border so, in that color, as PlantUML
            // draws it.
            if let Some(styled) = token.strip_prefix("##[")
                && let Some((word, rest)) = styled.split_once(']')
            {
                match word.trim().to_ascii_lowercase().as_str() {
                    "dashed" => more.dash = Some((7.0, 5.0)),
                    "dotted" => more.dash = Some((2.0, 4.0)),
                    "bold" => more.thickness = Some(2.5),
                    "plain" => more.dash = None,
                    _ => {
                        return Err(Declined::at(
                            line,
                            format!("`##[{word}]` is not supported yet"),
                        ));
                    }
                }
                if !rest.is_empty() {
                    let (_, border) = read_color(&format!("##{rest}"), line)?;
                    line_color = border;
                }
                continue;
            }
            if Inline::written(&token) {
                // `#fill;line:color;line.dashed;text:color`, PlantUML's
                // inline style.
                let style = Inline::read(&token).map_err(|err| Declined::at(line, err))?;
                if style.fill.is_some() {
                    fill = style.fill;
                }
                if style.line.is_some() {
                    line_color = style.line;
                }
                if style.text.is_some() {
                    more.text = style.text;
                }
                match style.dash {
                    Some(InlineDash::Dashed) => more.dash = Some((7.0, 5.0)),
                    Some(InlineDash::Dotted) => more.dash = Some((2.0, 4.0)),
                    None => {}
                }
                if style.bold {
                    more.thickness = Some(2.5);
                }
                continue;
            }
            let (next_fill, next_line) = read_color(&token, line)?;
            if next_fill.is_some() {
                fill = next_fill;
            }
            if next_line.is_some() {
                line_color = next_line;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    Ok((out, fill, line_color, more))
}

/// A note's `of` part, `Foo #pink;line:red : words`, cut into the
/// element's name, its colors and its words. The colors follow the name
/// after a space and come before the words: a `#` inside the name or the
/// words is theirs, a `:` inside a style the style's.
fn note_of(after: &str, line: usize) -> Result<(&str, &str, Option<&str>), Declined> {
    let (name, inline) = split_label(after);
    let name_end = crate::source::offset_in(after, name) + name.len();
    let Some(at) = colors_at(after).filter(|at| *at < name_end) else {
        return Ok((name, "", inline));
    };
    // Each color is a word of its own; one that is not a style ends at a
    // `:` as well, the words' (`#red: words`).
    let mut end = at;
    while after[end..].trim_start().starts_with('#') {
        end += after[end..].len() - after[end..].trim_start().len();
        let word = &after[end..];
        let word = &word[..word.find(char::is_whitespace).unwrap_or(word.len())];
        // `#red:wait;retry` is a color and words, `#pink;line:red` a style.
        end += match word.find(':') {
            Some(colon) if !crate::color::Inline::written(word) => colon,
            _ => word.len(),
        };
    }
    let left = after[end..].trim_start();
    let inline = if left.is_empty() {
        None
    } else if let Some(words) = left.strip_prefix(':') {
        Some(words.trim())
    } else {
        return Err(Declined::at(
            line,
            format!(
                "`{}` after a note's colors is not supported yet",
                snippet(left)
            ),
        ));
    };
    Ok((after[..at].trim(), &after[at..end], inline))
}

/// Where the colors after a name start: the first `#` after a space,
/// outside quotes.
fn colors_at(text: &str) -> Option<usize> {
    let mut quote = false;
    let mut after_space = false;
    for (at, c) in text.char_indices() {
        match c {
            '"' => quote = !quote,
            '#' if !quote && after_space => return Some(at),
            _ => {}
        }
        after_space = c.is_whitespace();
    }
    None
}

fn read_color(token: &str, line: usize) -> Result<(Option<String>, Option<String>), Declined> {
    if token.contains(';') {
        return Err(Declined::at(
            line,
            format!("`{token}` is not supported yet"),
        ));
    }
    if let Some(rest) = token.strip_prefix("##") {
        if crate::color::parse_gradient(rest).is_some() {
            return Err(Declined::at(
                line,
                format!("a gradient as a border (`{token}`) is not supported yet"),
            ));
        }
        crate::color::parse(rest).map_err(|err| Declined::at(line, err))?;
        return Ok((None, Some(rest.to_owned())));
    }
    // A fill may run from one color into another (`#red-green`), which
    // the drawer reads again.
    let rest = token.strip_prefix('#').unwrap_or(token);
    match crate::color::parse_gradient(rest) {
        Some(gradient) => {
            gradient.map_err(|err| Declined::at(line, err))?;
        }
        None => {
            crate::color::parse(rest).map_err(|err| Declined::at(line, err))?;
        }
    }
    Ok((Some(rest.to_owned()), None))
}

fn take_inherits(input: &str, line: usize) -> Result<(String, Vec<Inherit>), Declined> {
    let Some(cut) = inherit_at(input) else {
        return Ok((input.to_owned(), Vec::new()));
    };
    let name = input[..cut].trim().to_owned();
    let mut parents = Vec::new();
    parse_inherit_tail(&input[cut..], line, &mut parents)?;
    Ok((name, parents))
}

fn inherit_at(input: &str) -> Option<usize> {
    let mut quote = false;
    let mut depth = 0i32;
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (b, c) = chars[i];
        if c == '"' {
            quote = !quote;
            i += 1;
            continue;
        }
        if !quote && matches!(c, '<' | '(' | '[') {
            depth += 1;
        }
        if !quote && matches!(c, '>' | ')' | ']') {
            depth -= 1;
        }
        if !quote && depth == 0 && word_at(&chars, i, "extends") {
            return Some(b);
        }
        if !quote && depth == 0 && word_at(&chars, i, "implements") {
            return Some(b);
        }
        i += 1;
    }
    None
}

/// Whether `word` (any case) starts at `chars[i]` as a whole word. Compares
/// in place: copying the rest of the line at every position made long
/// lines quadratic.
fn word_at(chars: &[(usize, char)], i: usize, word: &str) -> bool {
    if i > 0 && !chars[i - 1].1.is_whitespace() {
        return false;
    }
    let mut at = i;
    for w in word.chars() {
        match chars.get(at) {
            Some((_, c)) if c.eq_ignore_ascii_case(&w) => at += 1,
            _ => return false,
        }
    }
    chars
        .get(at)
        .is_none_or(|(_, c)| c.is_whitespace() || *c == ',')
}

fn parse_inherit_tail(mut rest: &str, line: usize, out: &mut Vec<Inherit>) -> Result<(), Declined> {
    while !rest.trim().is_empty() {
        let trimmed = rest.trim();
        let (dash, shape, word) = if starts_with_ci(trimmed, "extends")
            && phrase(trimmed, "extends").is_some()
        {
            (Dash::Solid, Shape::Class, "extends")
        } else if starts_with_ci(trimmed, "implements") && phrase(trimmed, "implements").is_some() {
            (Dash::Dashed, Shape::Interface, "implements")
        } else {
            return Err(Declined::at(
                line,
                format!("`{}` is not supported yet", snippet(trimmed)),
            ));
        };
        rest = phrase(trimmed, word).unwrap_or("").trim();
        let cut = inherit_at(rest).unwrap_or(rest.len());
        let list = &rest[..cut];
        for part in list.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let mut raw = raw_name(part, line)?;
            if raw.shape.is_none() {
                raw.shape = Some(shape);
            }
            out.push(Inherit { raw, dash });
        }
        rest = &rest[cut..];
    }
    Ok(())
}

fn split_alias(text: &str, line: usize) -> Result<(String, String, Option<Shape>), Declined> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Declined::at(line, "a declaration needs a name"));
    }
    if let Some((left, right)) = split_as(text) {
        let left_name = raw_name(left, line)?;
        let right_name = raw_name(right, line)?;
        let shape = left_name.shape.or(right_name.shape);
        let (key, label) = if quoted(left) && !quoted(right) {
            (right_name.key, left_name.label)
        } else if !quoted(left) && quoted(right) {
            (left_name.key, right_name.label)
        } else {
            (right_name.key, left_name.label)
        };
        return Ok((key, label, shape));
    }
    let raw = raw_name(text, line)?;
    Ok((raw.key, raw.label, raw.shape))
}

/// The type parameters a declared name ends in, `Map<K, V>` or
/// `C as "Crate"<T>`, apart from the name: as PlantUML has it, the class
/// is `Map`, and a line to `Map` is a line to it. `None` when the name
/// ends in none; a quoted name keeps its `<` and `>`.
fn take_generic(name: &str) -> (&str, Option<&str>) {
    let name = name.trim();
    let Some(inside) = name.strip_suffix('>') else {
        return (name, None);
    };
    let mut depth = 1usize;
    for (at, c) in inside.char_indices().rev() {
        match c {
            '>' => depth += 1,
            '<' => depth -= 1,
            '"' => return (name, None),
            _ => {}
        }
        if depth == 0 {
            let before = inside[..at].trim_end();
            let generic = inside[at + 1..].trim();
            if before.is_empty() || before.ends_with('<') || generic.is_empty() {
                return (name, None);
            }
            return (before, Some(generic));
        }
    }
    (name, None)
}

fn quoted(text: &str) -> bool {
    text.trim().starts_with('"')
}

fn split_as(text: &str) -> Option<(&str, &str)> {
    let mut quote = false;
    let mut depth = 0i32;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (b, c) = chars[i];
        if c == '"' {
            quote = !quote;
            i += 1;
            continue;
        }
        if !quote && matches!(c, '<' | '(' | '[') {
            depth += 1;
        }
        if !quote && matches!(c, '>' | ')' | ']') {
            depth -= 1;
        }
        if !quote && depth == 0 && c.is_whitespace() && word_at(&chars, next_word(&chars, i), "as")
        {
            let mut k = next_word(&chars, i);
            // step past "as"
            k += 2;
            while k < chars.len() && chars[k].1.is_whitespace() {
                k += 1;
            }
            if k < chars.len() {
                return Some((text[..b].trim(), text[chars[k].0..].trim()));
            }
        }
        i += 1;
    }
    None
}

fn next_word(chars: &[(usize, char)], i: usize) -> usize {
    let mut j = i;
    while j < chars.len() && chars[j].1.is_whitespace() {
        j += 1;
    }
    j
}

fn raw_name(token: &str, line: usize) -> Result<RawName, Declined> {
    let token = token.trim();
    if token.is_empty() {
        return Err(Declined::at(line, "a name is missing"));
    }
    if let Some(inner) = strip_wrap(token, '(', ')') {
        if inner.is_empty() || inner == "*" {
            return Err(Declined::at(line, "`(*)` is not a use case"));
        }
        return Ok(RawName {
            key: inner.to_owned(),
            label: inner.to_owned(),
            shape: Some(Shape::UseCase),
            quoted: false,
        });
    }
    if let Some(inner) = strip_wrap(token, '[', ']') {
        if inner.is_empty() {
            return Err(Declined::at(line, "a component needs a name"));
        }
        return Ok(RawName {
            key: inner.to_owned(),
            label: inner.to_owned(),
            shape: Some(Shape::Component),
            quoted: false,
        });
    }
    if let Some(inner) = strip_wrap(token, ':', ':') {
        if inner.is_empty() {
            return Err(Declined::at(line, "an actor needs a name"));
        }
        return Ok(RawName {
            key: inner.to_owned(),
            label: inner.to_owned(),
            shape: Some(Shape::Actor),
            quoted: false,
        });
    }
    if let Some(inner) = strip_wrap(token, '"', '"') {
        if inner.is_empty() {
            return Err(Declined::at(line, "a name is missing"));
        }
        return Ok(RawName {
            key: inner.to_owned(),
            label: inner.to_owned(),
            shape: None,
            quoted: true,
        });
    }
    if token
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '{' | '}'))
    {
        return Err(Declined::at(
            line,
            format!("`{token}` is not a name this diagram draws"),
        ));
    }
    Ok(RawName {
        key: token.to_owned(),
        label: token.to_owned(),
        shape: None,
        quoted: false,
    })
}

fn strip_wrap(token: &str, open: char, close: char) -> Option<&str> {
    token.strip_prefix(open)?.strip_suffix(close)
}

/// The line `text`, which starts at the byte `at` of the source's line
/// `line`.
fn relation(text: &str, line: usize, at: usize) -> Result<RawRel, Declined> {
    let (lead, label) = split_label(text);
    let (start, end) = find_arrow(lead)
        .ok_or_else(|| Declined::at(line, format!("`{}` is not supported yet", snippet(text))))?;
    let left = lead[..start].trim();
    let arrow = &lead[start..end];
    let right = lead[end..].trim();
    // `-class "Private" {`: a class with its visibility, not a line.
    if let Some(rest) = text.strip_prefix(['-', '#', '~', '+'])
        && ["class", "abstract", "interface", "enum"]
            .iter()
            .any(|word| keyword(rest, word).is_some())
    {
        return Err(Declined::at(
            line,
            format!(
                "a class's visibility before it (`{}`) is not supported yet",
                &text[..1 + rest.split_whitespace().next().unwrap_or("").len()]
            ),
        ));
    }
    if left.is_empty() || right.is_empty() {
        return Err(Declined::at(line, "a line needs two elements"));
    }
    let (from, from_card, near_style) = side(left, true, line)?;
    let (to, to_card, style) = side(right, false, line)?;
    if let Some(style) = near_style {
        return Err(Declined::at(
            line,
            format!(
                "a line's own style before it (`{}`) is not supported yet",
                snippet(&style)
            ),
        ));
    }
    let mut arrow = parse_arrow(arrow, line)?;
    // `A --> B #line:red;line.bold;text:red`: the line's own style.
    if let Some(style) = style {
        own_style(&mut arrow, &style, line)?;
    }
    // Where each multiplicity stands: the one before the arrow ends its
    // side, the one after it begins its side. A line turned keeps them
    // where they were written.
    let card = |value: String, side: &str, last: bool| {
        let quoted = format!("\"{value}\"");
        let found = if last {
            side.rfind(&quoted)
        } else {
            side.find(&quoted)
        };
        Text {
            col: found.map(|found| at + offset_in(text, side) + found + 1),
            value,
            line,
        }
    };
    Ok(RawRel {
        from,
        to,
        from_card: from_card.map(|value| card(value, left, true)),
        to_card: to_card.map(|value| card(value, right, false)),
        arrow,
        label: label.map(str::to_owned),
    })
}

/// A line's own style after its far end, as PlantUML reads it: its color
/// (`#red`, `#line:red`, the first of `#green;line.dashed`), dashed,
/// dotted or bold, and its words' color (`text:red`).
fn own_style(arrow: &mut ArrowBits, style: &str, line: usize) -> Result<(), Declined> {
    let read = Inline::read(style).map_err(|err| Declined::at(line, err))?;
    if let Some(color) = read.line.or(read.fill) {
        arrow.color = Some(color);
    }
    match read.dash {
        Some(InlineDash::Dashed) => arrow.dash = Dash::Dashed,
        Some(InlineDash::Dotted) => arrow.dash = Dash::Dotted,
        None => {}
    }
    if read.bold {
        arrow.bold = true;
    }
    if read.text.is_some() {
        arrow.style.text = read.text;
    }
    Ok(())
}

/// One end of a line: its name, its multiplicity, and the line's own
/// style written after it (`#line:red;line.bold`).
type Side_ = (RawName, Option<String>, Option<String>);

fn side(text: &str, card_last: bool, line: usize) -> Result<Side_, Declined> {
    let mut tokens = tokens_of(text, line)?;
    // `() "Clipboard"`: the interface a component diagram draws as a
    // circle, `()` before its name.
    let circle = tokens
        .iter()
        .position(|token| token == "()")
        .filter(|&at| at + 1 < tokens.len());
    if let Some(at) = circle {
        tokens.remove(at);
    }
    // A style after the line's far end (`(bar) #line:red;line.bold`); a
    // `#` alone is an end the arrow did not take in.
    let style = tokens
        .iter()
        .skip(1)
        .position(|token| token.starts_with('#') && token.len() > 1)
        .map(|at| tokens.remove(at + 1));
    let (mut name, card) = match tokens.as_slice() {
        [name] => (raw_name(name, line)?, None),
        [name, card] if card_last => (raw_name(name, line)?, Some(cardinality(card, line)?)),
        [card, name] if !card_last => (raw_name(name, line)?, Some(cardinality(card, line)?)),
        _ => {
            return Err(Declined::at(
                line,
                format!("`{}` is not supported yet", snippet(text)),
            ));
        }
    };
    if circle.is_some() {
        name.shape = Some(Shape::Lollipop);
    }
    Ok((name, card, style))
}

fn cardinality(token: &str, line: usize) -> Result<String, Declined> {
    let token = token.trim();
    // What stands beside a name and is no cardinality: a style after the
    // line's ends, or an end the arrow did not take in.
    if token.starts_with('#') {
        return Err(Declined::at(
            line,
            format!(
                "a line's own style (`{}`) is not supported yet",
                snippet(token)
            ),
        ));
    }
    if !token.is_empty() && token.chars().all(|c| "}{|o#x+^<>*".contains(c)) {
        return Err(Declined::at(
            line,
            format!("the line end `{token}` is not drawn yet"),
        ));
    }
    if let Some(quoted) = token
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .filter(|s| !s.is_empty())
    {
        return Ok(quoted.to_owned());
    }
    // A multiplicity written bare (`1`, `*`, `0..1`) wants its quotes;
    // anything else beside an end is a part of the line not drawn yet.
    let multiplicity = !token.is_empty()
        && token
            .chars()
            .all(|c| c.is_ascii_digit() || "*.,n".contains(c));
    Err(Declined::at(
        line,
        if multiplicity {
            "a cardinality needs quotes".to_owned()
        } else {
            format!(
                "`{}` beside a line's end is not supported yet",
                snippet(token)
            )
        },
    ))
}

fn tokens_of(text: &str, line: usize) -> Result<Vec<String>, Declined> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '"' || c == '(' || c == '[' || c == ':' {
            let close = match c {
                '"' => '"',
                '(' => ')',
                '[' => ']',
                _ => ':',
            };
            i += 1;
            let mut end = None;
            while i < chars.len() {
                if chars[i].1 == close {
                    end = Some(chars[i].0 + chars[i].1.len_utf8());
                    i += 1;
                    break;
                }
                i += 1;
            }
            let Some(end) = end else {
                return Err(Declined::at(line, "a name is not closed"));
            };
            out.push(text[start..end].to_owned());
            continue;
        }
        i += 1;
        while i < chars.len() {
            let ch = chars[i].1;
            if ch.is_whitespace() {
                break;
            }
            if ch == '<' {
                i += 1;
                while i < chars.len() && chars[i].1 != '>' {
                    i += 1;
                }
                if i < chars.len() {
                    i += 1;
                }
                continue;
            }
            i += 1;
        }
        let end = if i < chars.len() {
            chars[i].0
        } else {
            text.len()
        };
        out.push(text[start..end].to_owned());
    }
    Ok(out)
}

fn split_label(text: &str) -> (&str, Option<&str>) {
    let mut quote = false;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (b, c) = chars[i];
        if c == '"' {
            quote = !quote;
            i += 1;
            continue;
        }
        // `:User:` where a name starts is an actor, not the label's colon.
        if !quote
            && c == ':'
            && name_starts_after(&text[..b])
            && let Some(end) = chars[i + 1..].iter().position(|(_, ch)| *ch == ':')
            && end > 0
        {
            i += end + 2;
            continue;
        }
        // `#line:red` is a line's own style, not its label.
        let word = text[..b].rfind(char::is_whitespace).map_or(0, |at| at + 1);
        if !quote && c == ':' && text[word..b].starts_with('#') {
            i += 1;
            continue;
        }
        // `A::count` names a member, not a label: both colons go by.
        if !quote && c == ':' && chars.get(i + 1).is_some_and(|(_, n)| *n == ':') {
            i += 2;
            continue;
        }
        if !quote && c == ':' && chars.get(i + 1).is_none_or(|(_, n)| *n != ':') {
            let label = text[b + 1..].trim();
            return (text[..b].trim(), Some(label));
        }
        i += 1;
    }
    (text.trim(), None)
}

/// Whether a name may start after `before`: at the start of the line or
/// right after an arrow. Elsewhere a colon starts the label, so
/// `A --> B : at 10:30` keeps its time.
fn name_starts_after(before: &str) -> bool {
    let mut chars = before.trim_end().chars().rev();
    match chars.next() {
        None | Some('-' | '.' | '>') => true,
        Some('|' | '*' | 'o' | 'O' | 'x' | 'X' | '#' | '+' | '^' | '}' | '{' | '\\' | '/') => {
            matches!(chars.next(), Some('-' | '.'))
        }
        _ => false,
    }
}

fn find_arrow(text: &str) -> Option<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut i = 0;
    let mut quote = false;
    // Past a `:` that closes nothing or stands before a space: a label,
    // or a member's words (`A : x : y`); an actor's (`:User:`) is a name.
    let mut words = false;
    while i < chars.len() {
        let (b, c) = chars[i];
        if quote {
            if c == '"' {
                quote = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            quote = true;
            i += 1;
            continue;
        }
        if c == ':' && chars.get(i + 1).is_none_or(|(_, ch)| ch.is_whitespace()) {
            words = true;
        }
        if c == '(' || (c == '[' && !bracket_is_arrow(&chars, i)) || c == ':' {
            let close = match c {
                '(' => ')',
                '[' => ']',
                _ => ':',
            };
            if let Some(end) = chars[i + 1..].iter().position(|(_, ch)| *ch == close) {
                i += end + 2;
                continue;
            }
            words |= c == ':';
        }
        // A dot alone between two names is a dotted line (`A . B`), as
        // one in a name is not (`net.Socket`).
        let lone = c == '.'
            && !words
            && i > 0
            && chars[i - 1].1.is_whitespace()
            && chars.get(i + 1).is_some_and(|(_, ch)| ch.is_whitespace());
        if lone {
            return Some((b, b + 1));
        }
        if arrow_at(&chars, i) {
            return Some((b, consume_arrow(text, &chars, i)));
        }
        i += 1;
    }
    None
}

fn bracket_is_arrow(chars: &[(usize, char)], i: usize) -> bool {
    matches!(
        chars.get(i + 1).map(|(_, c)| *c),
        Some('#' | '-' | '.' | '<' | '>' | 'o' | 'O' | 'x' | 'X')
    )
}

fn arrow_at(chars: &[(usize, char)], i: usize) -> bool {
    let c = chars[i].1;
    let next = chars.get(i + 1).map(|(_, ch)| *ch);
    match c {
        '-' => true,
        '.' => matches!(next, Some('.' | '-' | '>')) || dotted_direction(chars, i + 1),
        '<' => matches!(next, Some('-' | '|' | '.' | '>')),
        '*' | 'o' | 'O' | 'x' | 'X' => matches!(next, Some('-' | '.')),
        _ => false,
    }
}

fn consume_arrow(text: &str, chars: &[(usize, char)], mut i: usize) -> usize {
    while i < chars.len() {
        let c = chars[i].1;
        if c == '[' {
            let Some(end) = chars[i + 1..].iter().position(|(_, ch)| *ch == ']') else {
                break;
            };
            i += end + 2;
            continue;
        }
        if matches!(c, '-' | '.' | '<' | '>' | '*' | 'o' | 'O' | 'x' | 'X' | '|') {
            i += 1;
            continue;
        }
        if let Some(n) = direction_chars(chars, i) {
            i += n;
            continue;
        }
        break;
    }
    if i >= chars.len() {
        text.len()
    } else {
        chars[i].0
    }
}

fn parse_arrow(token: &str, line: usize) -> Result<ArrowBits, Declined> {
    if token.contains('+') {
        return Err(Declined::at(line, "nesting with `+` is not supported yet"));
    }
    // A direction says where the second box goes (docs/adr/0027).
    let toward = direction_word(token);
    let mut rest = token.to_owned();
    if let Some(word) = toward {
        let lower = rest.to_ascii_lowercase();
        if let Some(at) = lower.find(word) {
            rest.replace_range(at..at + word.len(), "");
        }
    }
    let from_head = take_prefix(&mut rest, line)?;
    let to_head = take_suffix(&mut rest, line)?;
    let mut dash = if rest.contains('.') {
        Dash::Dashed
    } else {
        Dash::Solid
    };
    let mut bold = false;
    let mut hidden = false;
    let mut color = None;
    let mut thickness = None;
    while let Some(start) = rest.find('[') {
        let Some(end) = rest.find(']') else {
            return Err(Declined::at(line, "an arrow bracket has no `]`"));
        };
        let inside = rest[start + 1..end].to_owned();
        if inside.trim().eq_ignore_ascii_case("hidden") {
            hidden = true;
        } else {
            apply_bracket(
                &inside,
                (&mut dash, &mut bold, &mut thickness),
                &mut color,
                line,
            )?;
        }
        rest.replace_range(start..end + 1, "");
    }
    if rest.chars().any(|c| c != '-' && c != '.')
        || (rest.is_empty() && from_head == Head::None && to_head == Head::None)
    {
        return Err(Declined::at(
            line,
            format!("`{token}` is not a line this diagram draws"),
        ));
    }
    // One dash or dot and no word: the two side by side ([`BESIDE`]).
    let toward = toward.or((rest.chars().count() == 1).then_some(BESIDE));
    Ok(ArrowBits {
        from_head,
        to_head,
        dash,
        bold,
        hidden,
        toward,
        color,
        style: super::LineStyle {
            thickness,
            text: None,
        },
    })
}

/// `up` inside `-up->`, not a name that merely follows an arrow.
/// Sets the direction of `group` and of every group inside it.
fn run_all(group: &mut Group, direction: Direction) {
    group.direction = direction;
    for node in &mut group.nodes {
        run_all(&mut node.children, direction);
    }
}

/// `.up.>`, `.left.`: a dotted arrow with its direction. What follows the
/// second dot is a head or the end of the arrow, never more of a name:
/// `com.up.Service` is one name.
fn dotted_direction(chars: &[(usize, char)], i: usize) -> bool {
    let Some(n) = direction_chars(chars, i) else {
        return false;
    };
    if chars.get(i + n).is_none_or(|(_, c)| *c != '.') {
        return false;
    }
    let at = |k: usize| chars.get(i + n + k).map(|(_, c)| *c);
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    match at(1) {
        // A head of a letter (`.up.o`, `.up.x`) when no name goes on.
        Some('o' | 'O' | 'x' | 'X') => !word(at(2)),
        c => !word(c),
    }
}

fn direction_chars(chars: &[(usize, char)], i: usize) -> Option<usize> {
    for word in ["down", "left", "right", "up"] {
        let letters: Vec<char> = word.chars().collect();
        if chars.len() < i + letters.len() {
            continue;
        }
        let matches = letters
            .iter()
            .enumerate()
            .all(|(k, letter)| chars[i + k].1.eq_ignore_ascii_case(letter));
        if !matches {
            continue;
        }
        let after = chars.get(i + letters.len()).map(|(_, c)| *c);
        if after.is_none_or(|c| {
            matches!(
                c,
                '-' | '.' | '<' | '>' | '[' | '|' | '*' | 'o' | 'O' | 'x' | 'X'
            )
        }) {
            return Some(letters.len());
        }
    }
    None
}

fn direction_word(arrow: &str) -> Option<&'static str> {
    let lower = arrow.to_ascii_lowercase();
    for word in ["up", "down", "left", "right"] {
        // `-up->`, and dotted `.up.>` as PlantUML reads it too.
        if lower.contains(&format!("-{word}"))
            || lower.contains(&format!("{word}-"))
            || lower.contains(&format!(".{word}."))
            || lower.contains(&format!("[{word}"))
        {
            return Some(word);
        }
    }
    None
}

fn take_prefix(rest: &mut String, line: usize) -> Result<Head, Declined> {
    if rest.starts_with('x') || rest.starts_with('X') {
        return Err(Declined::at(line, "an `x` arrow head is not supported yet"));
    }
    let head = if let Some(tail) = rest.strip_prefix("<|") {
        *rest = tail.to_owned();
        Head::Triangle
    } else if let Some(tail) = rest.strip_prefix('<') {
        *rest = tail.to_owned();
        Head::Arrow
    } else if let Some(tail) = rest.strip_prefix('*') {
        *rest = tail.to_owned();
        Head::DiamondFilled
    } else if rest.starts_with('o') || rest.starts_with('O') {
        *rest = rest[1..].to_owned();
        Head::Diamond
    } else {
        Head::None
    };
    Ok(head)
}

fn take_suffix(rest: &mut String, line: usize) -> Result<Head, Declined> {
    if rest.ends_with('x') || rest.ends_with('X') {
        return Err(Declined::at(line, "an `x` arrow head is not supported yet"));
    }
    let head = if let Some(tail) = rest.strip_suffix("|>") {
        *rest = tail.to_owned();
        Head::Triangle
    } else if let Some(tail) = rest.strip_suffix('>') {
        *rest = tail.to_owned();
        Head::Arrow
    } else if let Some(tail) = rest.strip_suffix('*') {
        *rest = tail.to_owned();
        Head::DiamondFilled
    } else if rest.ends_with('o') || rest.ends_with('O') {
        rest.pop();
        Head::Diamond
    } else if let Some(tail) = rest.strip_suffix("<|") {
        *rest = tail.to_owned();
        Head::Triangle
    } else if let Some(tail) = rest.strip_suffix('<') {
        *rest = tail.to_owned();
        Head::Arrow
    } else {
        Head::None
    };
    Ok(head)
}

fn apply_bracket(
    inside: &str,
    (dash, bold, thickness): (&mut Dash, &mut bool, &mut Option<f32>),
    color: &mut Option<String>,
    line: usize,
) -> Result<(), Declined> {
    if inside.trim().is_empty() {
        return Err(Declined::at(
            line,
            "an empty arrow bracket is not supported",
        ));
    }
    for part in inside.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let lower = part.to_ascii_lowercase();
        // `thickness=4`: the line that wide, as PlantUML draws it.
        if let Some(value) = lower.strip_prefix("thickness=") {
            let width: f32 = value
                .trim()
                .parse()
                .ok()
                .filter(|width: &f32| (0.0..=100.0).contains(width))
                .ok_or_else(|| Declined::at(line, format!("`[{part}]` names no width")))?;
            *thickness = Some(width);
            continue;
        }
        match lower.as_str() {
            "dashed" => *dash = Dash::Dashed,
            "dotted" => *dash = Dash::Dotted,
            "bold" => *bold = true,
            "plain" => *dash = Dash::Solid,
            "hidden" => {
                return Err(Declined::at(
                    line,
                    "`hidden` beside another word in an arrow's bracket is not supported yet",
                ));
            }
            _ if crate::color::parse(part).is_ok() => *color = Some(part.to_owned()),
            _ => {
                return Err(Declined::at(
                    line,
                    format!("`[{part}]` on an arrow is not supported yet"),
                ));
            }
        }
    }
    Ok(())
}

fn field_line(text: &str, line: usize) -> Result<Option<(RawName, Member)>, Declined> {
    if find_arrow(text).is_some() {
        return Ok(None);
    }
    let (left, label) = split_label(text);
    let Some(body) = label else {
        return Ok(None);
    };
    if body.is_empty() {
        return Err(Declined::at(line, "a member needs a text"));
    }
    if left.split_whitespace().count() != 1 {
        return Ok(None);
    }
    let mut slot = Slot::Attribute;
    let Some(member) = member_from(body, line, &mut slot)? else {
        return Err(Declined::at(line, "a member needs a text"));
    };
    Ok(Some((raw_name(left, line)?, member)))
}

fn member_from(text: &str, line: usize, slot: &mut Slot) -> Result<Option<Member>, Declined> {
    // `+{static} count`: the visibility may stand before the marks too,
    // as PlantUML reads it; it is shown before the name all the same.
    let trimmed = text.trim_start();
    let (visibility, marked) = match trimmed.chars().next() {
        Some(v @ ('+' | '-' | '#' | '~')) if trimmed[1..].trim_start().starts_with('{') => {
            (Some(v), &trimmed[1..])
        }
        _ => (None, trimmed),
    };
    let (mut italic, mut underline, forced, rest) = classifiers(marked, line)?;
    if let Some(forced) = forced {
        *slot = forced;
    }
    // `{static}` and `{abstract}` may stand anywhere in the member, as
    // PlantUML reads them (`+void {abstract} start()`): taken out, the
    // words around them close up.
    let (rest, (inner_italic, inner_underline)) = marks_within(rest);
    italic |= inner_italic;
    underline |= inner_underline;
    let rest = match visibility {
        Some(v) => format!("{v}{}", rest.trim_start()),
        None => rest,
    };
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok(None);
    }
    if rest.contains(['{', '}']) {
        return Err(Declined::at(
            line,
            format!("`{}` is not supported yet", snippet(text)),
        ));
    }
    if let Some(dash) = rule_of(rest) {
        return Ok(Some(Member::Rule(dash)));
    }
    if let Some(title) = heading_of(rest) {
        return Ok(Some(Member::Heading(text_at(title, line))));
    }
    let use_slot = if rest.contains('(') {
        Slot::Method
    } else {
        *slot
    };
    Ok(Some(Member::Text {
        text: text_at(rest, line),
        italic,
        underline,
        slot: use_slot,
    }))
}

/// `text` without the `{static}`, `{classifier}` and `{abstract}` marks
/// it holds, in any case, the words around each closed up with one space;
/// whether any made it abstract (italic) and static (underlined).
fn marks_within(text: &str) -> (String, (bool, bool)) {
    let (mut italic, mut underline) = (false, false);
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}').map(|at| open + at) else {
            break;
        };
        let word = rest[open + 1..close].trim().to_ascii_lowercase();
        let mark = match word.as_str() {
            "static" | "classifier" => &mut underline,
            "abstract" => &mut italic,
            _ => {
                out.push_str(&rest[..=close]);
                rest = &rest[close + 1..];
                continue;
            }
        };
        *mark = true;
        let before = rest[..open].trim_end();
        out.push_str(before);
        rest = rest[close + 1..].trim_start();
        if !out.is_empty() && !rest.is_empty() && !out.ends_with(char::is_whitespace) {
            out.push(' ');
        }
    }
    out.push_str(rest);
    (out, (italic, underline))
}

fn classifiers(text: &str, line: usize) -> Result<(bool, bool, Option<Slot>, &str), Declined> {
    let mut italic = false;
    let mut underline = false;
    let mut forced = None;
    let mut rest = text;
    loop {
        let trimmed = rest.trim_start();
        let Some(body) = trimmed.strip_prefix('{') else {
            return Ok((italic, underline, forced, trimmed));
        };
        let Some(end) = body.find('}') else {
            return Err(Declined::at(line, "a member mark has no `}`"));
        };
        let word = body[..end].trim().to_ascii_lowercase();
        rest = body[end + 1..].trim_start();
        match word.as_str() {
            "static" | "classifier" => underline = true,
            "abstract" => italic = true,
            "field" => forced = Some(Slot::Attribute),
            "method" => forced = Some(Slot::Method),
            "" => {}
            _ => {
                return Err(Declined::at(
                    line,
                    format!("`{{{word}}}` on a member is not supported yet"),
                ));
            }
        }
    }
}

fn rule_of(text: &str) -> Option<Dash> {
    let text = text.trim();
    if text.chars().count() < 2 {
        return None;
    }
    if text.chars().all(|c| c == '-' || c == '=') {
        Some(Dash::Solid)
    } else if text.chars().all(|c| c == '.') {
        Some(Dash::Dashed)
    } else if text.chars().all(|c| c == '_') {
        Some(Dash::Solid)
    } else {
        None
    }
}

fn heading_of(text: &str) -> Option<&str> {
    let inner = text.trim().strip_prefix("==")?.strip_suffix("==")?.trim();
    (!inner.is_empty()).then_some(inner)
}

fn take_quoted(text: &str, line: usize) -> Result<(String, &str), Declined> {
    let rest = text.trim_start();
    let body = rest
        .strip_prefix('"')
        .ok_or_else(|| Declined::at(line, "a note needs a text"))?;
    let Some(end) = body.find('"') else {
        return Err(Declined::at(line, "a note has no closing quote"));
    };
    Ok((body[..end].to_owned(), body[end + 1..].trim()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::hold::Word;

    fn lines_of(source: &str) -> Vec<crate::source::Line<'_>> {
        crate::source::body(source).unwrap().1
    }

    fn diagram(source: &str) -> Diagram {
        parse(&lines_of(source)).unwrap_or_else(|err| panic!("{err:?}"))
    }

    fn declined(source: &str) -> crate::Declined {
        parse(&lines_of(source)).unwrap_err()
    }

    #[test]
    fn a_dot_alone_between_names_is_a_dotted_line() {
        let d = diagram(
            "@startuml\nA . B\nnet.Socket --> A\nA : x . y\nA : x : y . z\n:User: . B\n@enduml\n",
        );
        // An actor makes it a description diagram: `net.Socket` stays one
        // box, as PlantUML reads it.
        let keys: Vec<&str> = d.root.nodes.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(keys, ["A", "B", "net.Socket", "User"]);
        assert_eq!(d.root.edges[0].dash, Dash::Dashed);
        assert_eq!(d.root.edges.len(), 3);
        // After a member's colon the dot is the member's.
        assert_eq!(d.root.nodes[0].members.len(), 2);
    }

    /// A line of one dash or dot and no word holds its second box beside
    /// the first, the two in the order written, as PlantUML lays them;
    /// a style in brackets leaves it one, two dashes run along the flow.
    #[test]
    fn one_dash_lays_its_ends_side_by_side() {
        for arrow in [
            "<|-", "-|>", "-", ".", "*-", "<-", "->", ".>", "-[#red]>", "o-",
        ] {
            let d = diagram(&format!(
                "@startuml\nclass A\nclass B\nA {arrow} B\n@enduml\n"
            ));
            let edge = &d.root.edges[0];
            assert_eq!(
                (edge.from, edge.to, edge.level),
                (0, 1, Some(true)),
                "{arrow}"
            );
        }
        for arrow in ["<|--", "--", "-[#red]-", "-down->", ".."] {
            let d = diagram(&format!(
                "@startuml\nclass A\nclass B\nA {arrow} B\n@enduml\n"
            ));
            assert_eq!(d.root.edges[0].level, None, "{arrow}");
        }
        // Hidden, it holds the two as `-right-` does.
        let d = diagram("@startuml\nclass A\nclass B\nA -[hidden]> B\n@enduml\n");
        assert!(d.root.edges.is_empty() && d.root.held.len() == 1);
        // Between boxes of different packages, a line like any other.
        let d = diagram("@startuml\npackage p {\n  class A\n}\nclass B\nA -> B\n@enduml\n");
        assert_eq!(d.root.crossings.len(), 1);
    }

    #[test]
    fn a_name_like_a_sequence_keyword_before_an_arrow_is_a_name() {
        // Classes named Box and Loop: a class diagram, as PlantUML reads it.
        for source in [
            "@startuml\nclass Box {\n  +width : int\n}\nclass Item\nBox \"1\" *-- \"many\" Item : holds\n@enduml\n",
            "@startuml\nclass Item\nBox <|-- Crate\nBox o-- Item\n@enduml\n",
            "@startuml\nclass Loop\nLoop <|-- Alt\n@enduml\n",
        ] {
            assert!(is_structure(&lines_of(source)), "{source}");
            diagram(source);
        }
        // Participants named Loop and Box send messages; a box is still one.
        crate::render(
            "@startuml\nLoop -> Bob : hi\nBob --> Loop : back\nBox -> Bob : too\n@enduml\n",
        )
        .unwrap();
        let boxed = "@startuml\nbox \"Shop\" #LightBlue\nparticipant Cart\nend box\nCart -> Bob : hi\n@enduml\n";
        assert!(!is_structure(&lines_of(boxed)));
        crate::render(boxed).unwrap();
    }

    #[test]
    fn a_generic_class_is_one_class_by_its_name() {
        // Declared before or after the line that names it.
        for source in [
            "@startuml\nclass Crate<T> {\n  +items : List<T>\n}\nStore --> Crate : holds\n@enduml\n",
            "@startuml\nStore --> Crate : holds\nclass Crate<T>\n@enduml\n",
        ] {
            let d = diagram(source);
            let keys: Vec<&str> = d.root.nodes.iter().map(|n| n.key.as_str()).collect();
            assert_eq!(
                keys.iter().filter(|k| k.starts_with("Crate")).count(),
                1,
                "{keys:?}"
            );
            let crate_ = d.root.nodes.iter().find(|n| n.key == "Crate").unwrap();
            assert_eq!(crate_.label.value, "Crate<T>");
        }
        // Parameters with spaces and bounds, an alias, nested ones.
        let d = diagram(
            "@startuml\nclass Map<K, V>\nclass C as \"Crate\"<T>\ninterface Repo<T extends Item>\nclass Box<List<T>>\nMap --> C\nRepo --> Map\nBox --> Repo\n@enduml\n",
        );
        let shown: Vec<(&str, &str)> = d
            .root
            .nodes
            .iter()
            .map(|n| (n.key.as_str(), n.label.value.as_str()))
            .collect();
        assert_eq!(
            shown,
            [
                ("Map", "Map<K, V>"),
                ("C", "Crate<T>"),
                ("Repo", "Repo<T extends Item>"),
                ("Box", "Box<List<T>>")
            ]
        );
        // A quoted name keeps its brackets; a stereotype is no parameter.
        assert_eq!(take_generic("\"Crate<T>\""), ("\"Crate<T>\"", None));
        assert_eq!(take_generic("Crate"), ("Crate", None));
        assert_eq!(take_generic("<T>"), ("<T>", None));
    }

    #[test]
    fn a_name_with_a_direction_word_is_no_dotted_arrow() {
        // `.up.` inside a package name is part of the name.
        diagram("@startuml\nclass com.up.Service\nclass Db\ncom.up.Service --> Db\n@enduml\n");
        diagram("@startuml\nclass A\nclass B\nA .up.> B\nA .left.o B\nB .down. A\n@enduml\n");
    }

    #[test]
    fn the_boxes_of_together_stand_in_one_rank_in_the_order_written() {
        let block = "together {\n  rectangle web\n  rectangle api\n  database db\n}\n";
        for direction in ["", "left to right direction\n"] {
            // `together` first or after a box: a structure diagram both.
            for (before, after) in [("rectangle top\n", ""), ("", "rectangle top\n")] {
                let source = format!(
                    "@startuml\n{direction}{before}{block}{after}rectangle low\ntop --> api\napi --> low\n@enduml\n"
                );
                let (_, map) = crate::render_mapped(&source, None).unwrap();
                let at = |name: &str| {
                    map.places
                        .iter()
                        .find(|p| p.name == name)
                        .unwrap_or_else(|| panic!("{name}: {:?}", map.places))
                        .rect
                };
                let (web, api, db) = (at("web"), at("api"), at("db"));
                let flat = direction.is_empty();
                let across = |r: crate::graph::Rect| if flat { r.x } else { r.y };
                let rank = |r: crate::graph::Rect| {
                    if flat {
                        r.y + r.h / 2.0
                    } else {
                        r.x + r.w / 2.0
                    }
                };
                assert!(
                    (rank(web) - rank(api)).abs() < 1.0 && (rank(api) - rank(db)).abs() < 1.0,
                    "{source}{web:?} {api:?} {db:?}"
                );
                assert!(
                    across(web) < across(api) && across(api) < across(db),
                    "{source}"
                );
            }
        }
    }

    #[test]
    fn a_hidden_line_places_its_boxes_and_a_direction_says_where() {
        let d = diagram(
            "rectangle a\nrectangle b\nrectangle c\nrectangle d\nrectangle e\na --> b\na -[hidden]-> c : unseen\na -[hidden]right- d\nb -[hidden]left- e\nb -[hidden]up- c\n",
        );
        let told: Vec<(usize, usize, bool)> = d
            .root
            .edges
            .iter()
            .map(|edge| (edge.from, edge.to, edge.hidden))
            .collect();
        // `up` runs against the flow: from c to b.
        assert_eq!(told, [(0, 1, false), (0, 2, true), (2, 1, true)]);
        let held: Vec<(usize, usize, bool)> = d
            .root
            .held
            .iter()
            .map(|held| (held.index, held.beside, held.word == Word::After))
            .collect();
        assert_eq!(held, [(3, 0, true), (4, 1, false)]);
        // Where the flow runs to the right, `down` is across it.
        let across = diagram(
            "left to right direction\nrectangle a\nrectangle b\na -[hidden]down- b\na -[hidden]right- b\n",
        );
        assert_eq!(across.root.held.len(), 1);
        assert_eq!(across.root.edges.len(), 1);
        // A line that is drawn runs within the rank, or along the flow:
        // against it from its second box to its first, its head with it.
        let drawn = diagram(
            "rectangle a\nrectangle b\nrectangle c\na -right-> b : beside\na <-left- c\nb -up-> a\nb -down-> c\n",
        );
        let told: Vec<(usize, usize, Option<bool>, Head, Head)> = drawn
            .root
            .edges
            .iter()
            .map(|edge| (edge.from, edge.to, edge.level, edge.from_head, edge.to_head))
            .collect();
        assert_eq!(
            told,
            [
                (0, 1, Some(true), Head::None, Head::Arrow),
                (0, 2, Some(false), Head::Arrow, Head::None),
                (0, 1, None, Head::Arrow, Head::None),
                (1, 2, None, Head::None, Head::Arrow),
            ]
        );
        assert!(drawn.root.held.is_empty());
        let err = declined("package p {\n  rectangle a\n}\nrectangle b\na -right-> b\n");
        assert!(err.reason.contains("different packages"), "{}", err.reason);
        // Along the flow it is a line like any other, out of a package too.
        let out =
            diagram("package p {\n  rectangle a\n}\nrectangle c\na -down-> c\nc <-up- a : back\n");
        let told: Vec<(&[usize], &[usize], Head, Head)> = out
            .root
            .crossings
            .iter()
            .map(|c| (&c.from[..], &c.to[..], c.arrow.from_head, c.arrow.to_head))
            .collect();
        assert_eq!(
            told,
            [
                (&[0, 0][..], &[1][..], Head::None, Head::Arrow),
                (&[0, 0][..], &[1][..], Head::None, Head::Arrow),
            ]
        );
    }

    #[test]
    fn an_inline_style_paints_the_border_and_the_words() {
        let d = diagram(
            "rectangle \"Core\" as core #F0EAFB;line.dashed;line:8250C4;text:navy\nrectangle b #line.bold;line:red\nrectangle c #line.dotted\n",
        );
        let [core, b, c] = &d.root.nodes[..] else {
            panic!("three boxes");
        };
        assert_eq!(core.fill.as_deref(), Some("F0EAFB"));
        assert_eq!(core.line_color.as_deref(), Some("8250C4"));
        assert_eq!(core.text_color.as_deref(), Some("navy"));
        assert_eq!(core.border_dash, Some((7.0, 5.0)));
        assert_eq!((b.fill.as_deref(), b.thickness), (None, Some(2.5)));
        assert_eq!(b.line_color.as_deref(), Some("red"));
        assert_eq!(c.border_dash, Some((2.0, 4.0)));
        let err = declined("rectangle a #white;line.wavy\n");
        assert_eq!(err.reason, "`line.wavy` in a style is not supported yet");
        assert!(crate::render("rectangle a #F0EAFB;line.dashed;line:8250C4\n").is_ok());
    }

    fn element<'a>(group: &'a Group, key: &str) -> &'a Element {
        group
            .nodes
            .iter()
            .find(|node| node.key == key)
            .unwrap_or_else(|| panic!("no {key}"))
    }

    #[test]
    fn detection_leaves_the_other_diagrams_alone() {
        assert!(matches!(
            crate::parse("@startuml\nactor User\nparticipant Web\nUser -> Web : hi\n@enduml"),
            Ok(crate::Diagram::Sequence(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\ndatabase Sessions\nUser -> Sessions : id\n@enduml"),
            Ok(crate::Diagram::Sequence(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\n[*] --> Idle\n@enduml"),
            Ok(crate::Diagram::State(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\nstart\n:go;\nstop\n@enduml"),
            Ok(crate::Diagram::Activity(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\nclass Foo\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\n:User: --> (Login)\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\n[Web] --> [Api]\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\nnode Server\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
        assert!(matches!(
            crate::parse("@startuml\nobject order\n@enduml"),
            Ok(crate::Diagram::Structure(_))
        ));
    }

    #[test]
    fn a_class_called_like_a_keyword_may_start_a_line() {
        // `node`, `person`, `queue`: keywords, and the names of classes.
        let diagram = diagram(
            "@startuml\nclass Node\nclass Person\nclass Queue\nNode --> Node : parent\nPerson --> Queue : waits in\nQueue --> Node : holds\n@enduml\n",
        );
        assert_eq!(diagram.root.nodes.len(), 3);
        assert_eq!(diagram.root.edges.len(), 3);
        // Still a node when it declares one.
        let nodes = self::tests::diagram(
            "@startuml\nnode Server\nnode Client\nClient --> Server\n@enduml\n",
        );
        assert!(nodes.root.nodes.iter().all(|n| n.shape == Shape::Node));
    }

    #[test]
    fn an_entity_with_a_body_is_a_class_of_a_data_model() {
        let source = "@startuml\nentity Person {\n  id\n}\nentity Team {\n  name\n}\nPerson --> Team : in\nPerson --> Person : manages\n@enduml\n";
        assert!(matches!(
            crate::parse(source),
            Ok(crate::Diagram::Structure(_))
        ));
        let diagram = diagram(source);
        let person = element(&diagram.root, "Person");
        assert_eq!(person.shape, Shape::Entity);
        assert_eq!(person.spot.as_ref().map(|s| s.kind), Some(SpotKind::Entity));
        assert_eq!(person.members.len(), 1);
        assert_eq!(diagram.root.edges.len(), 2);
        // Without a body an entity is a participant, as in PlantUML.
        assert!(matches!(
            crate::parse("@startuml\nentity Person\nPerson -> Team : join\n@enduml\n"),
            Ok(crate::Diagram::Sequence(_))
        ));
    }

    #[test]
    fn a_colon_after_a_name_starts_the_label() {
        let diagram = diagram(
            "@startuml\nclass A\nclass B\nA --> B : GET http://host\nA --> B : at 10:30\nclass User\nUser : password : String\n:Admin: --> (Login) : uses\n@enduml\n",
        );
        let labels: Vec<_> = diagram
            .root
            .edges
            .iter()
            .filter_map(|edge| edge.label.as_ref().map(|l| l.value.as_str()))
            .collect();
        assert_eq!(labels, ["GET http://host", "at 10:30", "uses"]);
        let user = element(&diagram.root, "User");
        assert!(matches!(
            &user.members[0],
            Member::Text { text, .. } if text.value == "password : String"
        ));
        element(&diagram.root, "Admin");
    }

    #[test]
    fn members_may_start_with_a_shape_word() {
        let diagram = diagram(
            "@startuml\nenum NodeType {\n  FILE\n  FOLDER\n}\nclass Tree {\n  file : File\n  node : Node\n  abstract void draw()\n}\n@enduml\n",
        );
        assert_eq!(element(&diagram.root, "NodeType").members.len(), 2);
        assert_eq!(element(&diagram.root, "Tree").members.len(), 3);
        let err = declined("@startuml\nclass A {\n  x : int\nclass B {\n}\n@enduml\n");
        assert_eq!(err.line, Some(3));
        let err = declined("@startuml\nclass A {\nnote left of A\n}\n@enduml\n");
        assert_eq!(err.line, Some(2));
    }

    /// REQ-055: the visibility may stand before a member's marks, as
    /// PlantUML reads it, and is shown before the name.
    #[test]
    fn a_visibility_before_the_marks_is_read_too() {
        let d = diagram(
            "@startuml\nclass Shape {\n  +{static} count : int\n  #{abstract} draw()\n  -{static}{abstract} odd()\n}\n@enduml\n",
        );
        let shape = element(&d.root, "Shape");
        let read: Vec<(String, bool, bool)> = shape
            .members
            .iter()
            .map(|m| match m {
                Member::Text {
                    text,
                    italic,
                    underline,
                    ..
                } => (text.value.clone(), *italic, *underline),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            read,
            [
                ("+count : int".to_owned(), false, true),
                ("#draw()".to_owned(), true, false),
                ("-odd()".to_owned(), true, true),
            ]
        );
    }

    /// `{static}` and `{abstract}` anywhere in a member, in any case, as
    /// PlantUML reads them: taken out, the words around them closed up.
    #[test]
    fn marks_stand_anywhere_in_a_member() {
        let d = diagram(
            "@startuml\nclass A {\n  +void {abstract} start(int timeout)\n  int {static} count\n  foo {ABSTRACT} bar\n  + {classifier} x\n}\n@enduml\n",
        );
        let read: Vec<(String, bool, bool)> = element(&d.root, "A")
            .members
            .iter()
            .map(|m| match m {
                Member::Text {
                    text,
                    italic,
                    underline,
                    ..
                } => (text.value.clone(), *italic, *underline),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            read,
            [
                ("+void start(int timeout)".to_owned(), true, false),
                ("int count".to_owned(), false, true),
                ("foo bar".to_owned(), true, false),
                ("+x".to_owned(), false, true),
            ]
        );
    }

    /// A note on a member (`A::counter`, `A::"start(int ms)"`) is on its
    /// element and that member; one without `of` on the element declared
    /// last.
    #[test]
    fn notes_on_members_and_on_the_last_element() {
        let d = diagram(
            "@startuml\nclass A {\n  {static} int counter\n  +void start(int ms)\n  +void start(Duration d)\n}\nnote right of A::counter\n  one\nend note\nnote left of A::\"start(Duration d)\"\n  two\nend note\nclass Foo\nnote left: on Foo\n@enduml\n",
        );
        let notes: Vec<(&str, Option<usize>, &str)> = d
            .root
            .notes
            .iter()
            .map(|n| {
                (
                    d.root.nodes[n.target].key.as_str(),
                    n.member,
                    n.text.value.as_str(),
                )
            })
            .collect();
        assert_eq!(
            notes,
            [
                ("A", Some(0), "one"),
                ("A", Some(2), "two"),
                ("Foo", None, "on Foo")
            ]
        );
        let err =
            declined("@startuml\nclass A\nnote right of A::nothing\n  x\nend note\n@enduml\n");
        assert!(err.reason.contains("no member `nothing`"), "{err:?}");
        // PlantUML takes a note on a member as a block only.
        let err = declined("@startuml\nclass A {\n  int x\n}\nnote right of A::x : y\n@enduml\n");
        assert!(
            err.refused && err.reason.contains("as a block only"),
            "{err:?}"
        );
        // A member by its name before its value, or after a type of `::`.
        let d = diagram(
            "@startuml\nclass A {\n  counter = 0\n  std::string name\n}\nnote right of A::counter\n  c\nend note\nnote right of A::name\n  n\nend note\n@enduml\n",
        );
        let members: Vec<Option<usize>> = d.root.notes.iter().map(|n| n.member).collect();
        assert_eq!(members, [Some(0), Some(1)]);
    }

    /// A class's name parts into the packages it stands in, as PlantUML
    /// reads it: at `.`, or at the separator set (`set separator ::`),
    /// not at all after `set separator none`; one package of the whole
    /// prefix under `!pragma useIntermediatePackages false`. A line's end
    /// names the same class by its whole name, an alias stays whole, and
    /// a component is not parted.
    #[test]
    fn a_class_name_parts_into_packages() {
        // The packages from the root down, by label, and their classes.
        fn tree(group: &Group, out: &mut Vec<String>, depth: usize) {
            for node in &group.nodes {
                out.push(format!("{}{}", "  ".repeat(depth), node.label.value));
                tree(&node.children, out, depth + 1);
            }
        }
        // Every line, at whatever depth it is kept.
        fn lines(group: &Group) -> usize {
            group.edges.len()
                + group.crossings.len()
                + group
                    .nodes
                    .iter()
                    .map(|n| lines(&n.children))
                    .sum::<usize>()
        }
        let read = |source: &str| {
            let d = diagram(source);
            let mut out = Vec::new();
            tree(&d.root, &mut out, 0);
            (out, lines(&d.root))
        };
        let (tree_a, _) = read(
            "@startuml\nclass a.b.C\npackage a.b {\n  class D\n}\nclass \"x.y\" as XY\n@enduml\n",
        );
        assert_eq!(tree_a, ["a", "  b", "    C", "    D", "x.y"]);
        // A component diagram's names stay whole, its packages' too.
        let (whole, _) =
            read("@startuml\npackage com.acme {\n  [Billing]\n}\nnode n\nn --> a.b\n@enduml\n");
        assert_eq!(whole, ["com.acme", "  Billing", "n", "a.b"]);
        let (tree_b, lines) =
            read("@startuml\npackage a.b {\n  class c\n}\nclass a.b.d\na.b.c --> a.b.d\n@enduml\n");
        assert_eq!(tree_b, ["a", "  b", "    c", "    d"]);
        assert_eq!(lines, 1);
        let (tree_c, _) =
            read("@startuml\nset separator ::\nclass X1::X2::foo\nclass p.q\n@enduml\n");
        assert_eq!(tree_c, ["X1", "  X2", "    foo", "p.q"]);
        let (tree_d, _) = read("@startuml\nset separator none\nclass a.b.C\n@enduml\n");
        assert_eq!(tree_d, ["a.b.C"]);
        let (tree_e, _) =
            read("@startuml\n!pragma useIntermediatePackages false\nclass A.B.C.D.Z\n@enduml\n");
        assert_eq!(tree_e, ["A.B.C.D", "  Z"]);
    }

    /// Names as PlantUML finds them once whole names part into packages:
    /// a declaration of `C` is no class made from `a.b.C`, nor is a line's
    /// end written before it or a parent; a line's short name is the one
    /// class of that name; `.C` is at the top; a class keeps its short
    /// name declared again by its whole one, and a generic its `<T>`.
    #[test]
    fn whole_names_and_short_ones() {
        let keys = |source: &str| {
            fn walk(group: &Group, out: &mut Vec<String>) {
                for node in &group.nodes {
                    out.push(format!("{}={}", node.key, node.label.value));
                    walk(&node.children, out);
                }
            }
            let mut out = Vec::new();
            walk(&diagram(source).root, &mut out);
            out
        };
        assert_eq!(
            keys("@startuml\nclass a.b.C\nclass C\nC --> a.b.C\n@enduml\n"),
            ["a=a", "a.b=b", "a.b.C=C", "C=C"]
        );
        // A line's boxes are made as it is read, as PlantUML makes them.
        assert_eq!(
            keys("@startuml\nC --> X\nclass a.b.C\n@enduml\n"),
            ["C=C", "X=X", "a=a", "a.b=b", "a.b.C=C"]
        );
        assert!(
            keys("@startuml\nclass net.Socket\nclass B extends Socket\n@enduml\n")
                .contains(&"Socket=Socket".to_owned())
        );
        assert_eq!(
            keys("@startuml\nclass net.Socket\nSocket --> A\n@enduml\n"),
            ["net=net", "net.Socket=Socket", "A=A"]
        );
        assert!(
            keys("@startuml\nclass a.C\nclass b.C\nC --> X\n@enduml\n").contains(&"C=C".to_owned())
        );
        assert_eq!(
            keys("@startuml\nclass Base\npackage p {\n  .Base <|-- K\n}\n@enduml\n"),
            ["Base=Base", "p=p", "K=K"]
        );
        assert_eq!(
            keys(
                "@startuml\npackage a.b {\n  class C\n}\nclass a.b.C #pink\nclass a.d<T>\n@enduml\n"
            ),
            ["a=a", "a.b=b", "C=C", "a.d=d<T>"]
        );
        // A class named `Set` is no `set` line.
        assert_eq!(
            keys("@startuml\nSet <|-- HashSet\n@enduml\n"),
            ["Set=Set", "HashSet=HashSet"]
        );
        // Under `::`, `X::Y` is a whole name, not a member.
        assert_eq!(
            keys("@startuml\nset separator ::\nX::Y --> Z::W\n@enduml\n"),
            ["X=X", "X::Y=Y", "Z=Z", "Z::W=W"]
        );
        // One package of the whole prefix only where it holds nothing else.
        assert_eq!(
            keys(
                "@startuml\n!pragma useIntermediatePackages false\nclass a.b.C\nclass a.E\n@enduml\n"
            ),
            ["a=a", "a.b=b", "a.b.C=C", "a.E=E"]
        );
        // A note and `hide` by a whole name through declared packages;
        // `together` across packages holds nothing, and draws.
        let d = diagram(
            "@startuml\npackage a.b {\n  class C\n}\nnote right of a.b.C : hi\nhide a.b.C\ntogether {\n  class a.D\n  class E\n}\n@enduml\n",
        );
        assert_eq!(d.root.nodes[0].children.nodes[0].children.notes.len(), 1);
        assert!(d.root.nodes[0].children.nodes[0].children.nodes[0].hidden);
        // `hide` of a whole name's last part passes by, as in PlantUML.
        let d = diagram("@startuml\nclass net.Socket\nhide Socket\n@enduml\n");
        assert!(!d.root.nodes[0].children.nodes[0].hidden);
        // What PlantUML refuses, or Omaspec does not draw yet.
        let err = declined("@startuml\nclass a.B\nclass a\n@enduml\n");
        assert!(err.reason.contains("is a package already"), "{err:?}");
        let err =
            declined("@startuml\nclass A\nclass B\nclass C\n(A, B) .. C\nnote left: x\n@enduml\n");
        assert!(err.reason.contains("association's point"), "{err:?}");
        // A note on a member outlasts `remove` of other boxes.
        let d = diagram(
            "@startuml\nclass A {\n  int x\n}\nclass B\nnote right of B : on B\nnote right of A::x\n  on x\nend note\nremove B\n@enduml\n",
        );
        let note = d
            .root
            .notes
            .iter()
            .find(|n| n.text.value == "on x")
            .unwrap();
        assert_eq!(
            (d.root.nodes[note.target].key.as_str(), note.member),
            ("A", Some(0))
        );
        let d = diagram(
            "@startuml\nclass A {\n  int x\n}\nnote right of A::x\n  on x\nend note\nremove A\n@enduml\n",
        );
        assert!(d.root.notes.is_empty());
    }

    /// Names belong to the packages they stand in, as PlantUML reads a
    /// class diagram: `class C` in two packages is two classes, a line
    /// finds its package's first, else the one of that name, and a line
    /// or a declaration that came first makes its own. Under `set
    /// separator none` and in a description diagram a name is the whole
    /// diagram's.
    #[test]
    fn names_belong_to_their_packages() {
        fn walk(group: &Group, at: &str, boxes: &mut Vec<String>, lines: &mut Vec<String>) {
            let name = |group: &Group, path: &[usize]| {
                let mut group = group;
                let mut parts = Vec::new();
                for &index in path {
                    parts.push(group.nodes[index].label.value.clone());
                    group = &group.nodes[index].children;
                }
                let inner = parts.join("/");
                if at.is_empty() {
                    inner
                } else {
                    format!("{at}/{inner}")
                }
            };
            for edge in &group.edges {
                lines.push(format!(
                    "{}>{}",
                    name(group, &[edge.from]),
                    name(group, &[edge.to])
                ));
            }
            for crossing in &group.crossings {
                lines.push(format!(
                    "{}>{}",
                    name(group, &crossing.from),
                    name(group, &crossing.to)
                ));
            }
            for node in &group.nodes {
                let here = if at.is_empty() {
                    node.label.value.clone()
                } else {
                    format!("{at}/{}", node.label.value)
                };
                boxes.push(here.clone());
                walk(&node.children, &here, boxes, lines);
            }
        }
        let read = |body: &str| {
            let d = diagram(&format!("@startuml\n{body}\n@enduml\n"));
            let (mut boxes, mut lines) = (Vec::new(), Vec::new());
            walk(&d.root, "", &mut boxes, &mut lines);
            boxes.sort();
            lines.sort();
            (boxes.join(" "), lines.join(" "))
        };
        let (a, b) = ("package a {\n  class C\n}\n", "package b {\n  class C\n}\n");
        assert_eq!(read(&format!("{a}{b}")).0, "a a/C b b/C");
        assert_eq!(read(&format!("set separator none\n{a}{b}")).0, "a a/C b");
        assert_eq!(
            read(&format!(
                "{a}package b {{\n  class C\n  C --> D\n}}\nC --> E"
            )),
            ("C E a a/C b b/C b/D".into(), "C>E b/C>b/D".into())
        );
        assert_eq!(
            read(&format!("{a}package b {{\n  C --> D\n}}")),
            ("a a/C b b/D".into(), "a/C>b/D".into())
        );
        assert_eq!(read(&format!("class C\n{b}")).0, "C b b/C");
        assert_eq!(read(&format!("{a}{b}a.C --> b.C")).1, "a/C>b/C");
        assert_eq!(
            read(&format!("C --> D\n{b}")),
            ("C D b b/C".into(), "C>D".into())
        );
        assert_eq!(
            read("package b {\n  C --> D\n}\nclass C"),
            ("C b b/C b/D".into(), "b/C>b/D".into())
        );
        assert_eq!(read(&format!("{a}{a}")).0, "a a/C");
        assert_eq!(
            read("package a {\n  class C {\n    x\n  }\n}\nclass a.C {\n  y\n}").0,
            "a a/C"
        );
        let (boxes, lines) = read(&format!("{b}note right of C : n\nC --> X"));
        assert_eq!((boxes.as_str(), lines.as_str()), ("X b b/C", "b/C>X"));
        // A line finds what a line before it made, as PlantUML makes it
        // when it reads that line (the guide's `Object` in two packages).
        assert_eq!(
            read("package P {\n  Object <|-- ArrayList\n}\npackage q {\n  Object <|-- Demo1\n}"),
            (
                "P P/ArrayList P/Object q q/Demo1".into(),
                "P/Object>P/ArrayList P/Object>q/Demo1".into()
            )
        );
        // Components are the whole diagram's.
        assert_eq!(
            read("package a {\n  [C]\n}\npackage b {\n  [C]\n}").0,
            "a a/C b"
        );
        // An alias's words name no box: the one `User` is b's.
        assert_eq!(
            read(
                "package a {\n  class \"User\" as U1\n}\npackage b {\n  class User\n}\nUser --> X"
            )
            .1,
            "b/User>X"
        );
        // A whole name is found through its packages alone.
        assert_eq!(
            read("package x {\n  class a.C\n}\na.C --> E").0,
            "E a a/C x x/a x/a/C"
        );
        // An actor under `allowmixing` is the whole diagram's.
        assert_eq!(
            read("allowmixing\nclass K\npackage a {\n  actor U\n}\npackage b {\n  actor U\n}").0,
            "K a a/U b"
        );
        // A line's boxes are made as it is read: a later line finds two,
        // and makes its own; a member line finds the one.
        assert_eq!(
            read("package P {\n  X --> A\n}\npackage Q {\n  class X\n}\npackage R {\n  X --> B\n}")
                .1,
            "P/X>P/A R/X>R/B"
        );
        assert_eq!(
            read("package R {\n  X --> B\n}\npackage P {\n  X : field\n}").0,
            "P R R/B R/X"
        );
        // A note finds its box from where it is written.
        for (body, on) in [
            (
                "package a {\n  class C\n}\npackage b {\n  class C\n  note right of C : n\n}",
                "b",
            ),
            (
                "package a {\n  class C\n}\npackage b {\n  class C\n  note right : n\n}",
                "b",
            ),
            (
                "package a {\n  class C\n}\nclass C\nnote right of C : n",
                "",
            ),
        ] {
            let d = diagram(&format!("@startuml\n{body}\n@enduml\n"));
            let group = match on {
                "b" => &d.root.nodes[1].children,
                _ => &d.root,
            };
            assert_eq!(group.notes.len(), 1, "{body}");
            assert_eq!(group.nodes[group.notes[0].target].key, "C", "{body}");
        }
        let d = diagram(
            "@startuml\npackage a {\n  class C\n}\npackage b {\n  class C {\n    int y\n  }\n  note right of C::y\n    n\n  end note\n}\n@enduml\n",
        );
        assert_eq!(d.root.nodes[1].children.notes[0].member, Some(0));
        // Packages of one name in two packages are two.
        assert_eq!(
            read("package a {\n  package util {\n    class X\n  }\n}\npackage b {\n  package util {\n    class Y\n  }\n}").0,
            "a a/util a/util/X b b/util b/util/Y"
        );
        // A parent is made as its declaration is read: a later line finds it.
        assert_eq!(
            read("class Dog extends Animal\npackage zoo {\n  Animal --> Keeper\n}"),
            (
                "Animal Dog zoo zoo/Keeper".into(),
                "Animal>Dog Animal>zoo/Keeper".into()
            )
        );
        // A whole name is found through its packages, from where it is
        // written too; a box of no class's kind by its name.
        assert!(
            read("class a.C\nclass a.D\n(a.C, a.D) .. K")
                .0
                .contains("a/C")
        );
        let d =
            diagram("@startuml\npackage x {\n  class a.C\n  note right of a.C : n\n}\n@enduml\n");
        assert_eq!(d.root.nodes[0].children.nodes[0].children.notes.len(), 1);
        assert_eq!(read("class C\nnote \"x\" as a.N\na.N .. C").0, "C x");
        // A note without `of` is on the box the line before it made.
        let d = diagram(
            "@startuml\npackage P {\n  class X\n}\npackage Q {\n  class X\n}\npackage R {\n  B --> X\n  note right : n\n}\n@enduml\n",
        );
        let r = &d.root.nodes[2].children;
        assert_eq!(r.nodes[r.notes[0].target].key, "X");
        // A note named as a class of another package is another box; of
        // its own package, PlantUML refuses it.
        assert!(
            read("package p {\n  class N\n}\nclass A\nnote \"x\" as N\nN .. A")
                .0
                .contains("p/N")
        );
        let source = "@startuml\nclass N\nnote \"x\" as N\n@enduml\n";
        let (_, lines) = crate::source::body(source).unwrap();
        assert!(parse(&lines).unwrap_err().refused);
        // A whole name from the top where its first package stands there.
        assert_eq!(
            read("class a.C\npackage x {\n  package a {\n    class C\n  }\n  a.C --> D\n}").1,
            "a/C>x/D"
        );
        // A note named as a box a line made: PlantUML refuses it.
        let source = "@startuml\nclass A\nA --> N\nnote \"x\" as N\n@enduml\n";
        let (_, lines) = crate::source::body(source).unwrap();
        assert!(parse(&lines).unwrap_err().refused);
        // A note named as a box of another package, of any kind, is
        // another box, as PlantUML reads it.
        for source in [
            "package p {\n  note \"a\" as N\n}\nnote \"b\" as N",
            "package p {\n  note \"a\" as N\n}\npackage q {\n  note \"b\" as N\n}",
            "allowmixing\nactor N\npackage p {\n  note \"x\" as N\n}",
            "package p {\n  package N {\n    class A\n  }\n}\nnote \"x\" as N",
        ] {
            diagram(&format!("@startuml\n{source}\n@enduml\n"));
        }
        // A package's label is no first package of a whole name.
        assert_eq!(
            read("package \"a\" as P {\n  class X\n}\npackage x {\n  package a {\n    class C\n  }\n  a.C --> D\n}").1,
            "x/a/C>x/D"
        );
        // A note on a line knows the lines to notes as they are placed.
        let d = diagram(
            "@startuml\nclass A\nclass B\nA --> B : first\nnote \"x\" as N\npackage p {\n  class N\n  class C\n  C --> N : inner\n}\nnote on link : which\n@enduml\n",
        );
        let p = d.root.nodes.iter().find(|n| n.key == "p").unwrap();
        assert!(d.root.edges[0].note.is_none());
        assert!(p.children.edges[0].note.is_some());
        let d = diagram(
            "@startuml\nclass A\nA --> B : first\npackage p {\n  note \"x\" as N\n}\np.N .. A\nnote on link : which\n@enduml\n",
        );
        let first = d.root.edges.iter().find(|e| e.label.is_some()).unwrap();
        assert!(first.note.is_some());
        // An association class's pair is found from where it is written.
        let d = diagram(
            "@startuml\npackage a {\n  class A\n  class B\n}\npackage b {\n  class A\n  class B\n  (A, B) .. K\n}\n@enduml\n",
        );
        assert!(d.root.nodes[0].children.edges.is_empty());
        assert!(
            d.root.nodes[1]
                .children
                .nodes
                .iter()
                .any(|n| n.shape == Shape::Point)
        );
    }

    /// A class diagram as PlantUML tells it from a description diagram: a
    /// class named like a keyword, a container's block of classes, and a
    /// label in brackets leave it one, its names parting; a box in
    /// brackets with spaces in its name makes it none.
    #[test]
    fn a_class_diagram_is_told_from_a_description_diagram() {
        let parted = |source: &str| {
            let d = diagram(&format!("@startuml\n{source}\n@enduml\n"));
            d.root.nodes.iter().any(|node| node.key == "a")
        };
        for source in [
            "class a.b.Node\nclass a.b.Edge\nNode --> Edge",
            "class a.b.C\nFile : name",
            "folder F {\n  class a.b.C\n}",
            "class a.b.C\nclass a.b.D\nC --> D : (optional)",
            "class a.b.C\nclass a.b.D\nC --> D : [guard]",
            "class a.b.C\nnote right of C : see (1)",
        ] {
            let d = diagram(&format!("@startuml\n{source}\n@enduml\n"));
            let found = |group: &Group| group.nodes.iter().any(|n| n.key == "a");
            let inside = d.root.nodes.iter().any(|n| found(&n.children));
            assert!(parted(source) || inside, "{source}");
        }
        for source in [
            "[Web Server] --> a.b.Api",
            "class a.b.C\nnode N",
            "a.b.C --> (Use the app)",
            ":Main Admin: --> a.b.C",
        ] {
            assert!(!parted(source), "{source}");
        }
        // `title : x` is a class's member here.
        let d = diagram("@startuml\ntitle : Hi\nA --> B : (optional)\n@enduml\n");
        assert!(d.root.nodes.iter().any(|node| node.key == "title"));
        // A quoted name with `::` is one name.
        let d = diagram("@startuml\n\"ns::Impl\" ..|> Api\n@enduml\n");
        assert!(d.root.nodes.iter().any(|node| node.key == "ns::Impl"));
        // A short name is found where its line stands, not later.
        let d = diagram("@startuml\nclass a.b.C\nD --> C\nclass C\n@enduml\n");
        let edge = &d.root.crossings[0];
        assert_eq!(edge.to.len(), 3, "{:?}", d.root.crossings);
        // `hide` by a package's name, not its label.
        let d = diagram(
            "@startuml\npackage \"My Pkg\" as mp {\n  class C\n  class D\n}\nhide mp.C\n@enduml\n",
        );
        let hidden: Vec<bool> = d.root.nodes[0]
            .children
            .nodes
            .iter()
            .map(|n| n.hidden)
            .collect();
        assert_eq!(hidden, [true, false]);
    }

    /// A note without `of` is on what was made last, as PlantUML counts
    /// it: an element a line names first, not one a member line adds to;
    /// after a note, on that note, which is not drawn yet.
    #[test]
    fn a_note_without_of_is_on_what_was_made_last() {
        let on = |source: &str| {
            let d = diagram(source);
            let note = d.root.notes.last().expect("a note");
            d.root.nodes[note.target].key.clone()
        };
        assert_eq!(
            on("@startuml\nclass A\nclass B\nA --> C\nnote left: x\n@enduml\n"),
            "C"
        );
        assert_eq!(
            on("@startuml\nclass A {\n  int x\n}\nclass B\nA : int y\nnote left: x\n@enduml\n"),
            "B"
        );
        let err = declined(
            "@startuml\nclass A\nnote right of A\n  one\nend note\nnote left: two\n@enduml\n",
        );
        assert!(err.reason.contains("a note on a note"), "{err:?}");
    }

    /// A note on a member stays on it when `hide` takes others away, and
    /// is on its element when its member is hidden itself.
    #[test]
    fn a_note_on_a_member_outlasts_hide() {
        let d = diagram(
            "@startuml\nclass A {\n  int a\n  void start()\n  void stop()\n}\nnote right of A::start\n  on start\nend note\nnote left of A::a\n  on a\nend note\nhide fields\n@enduml\n",
        );
        let a = element(&d.root, "A");
        let members: Vec<Option<&str>> = d
            .root
            .notes
            .iter()
            .map(|n| {
                n.member.map(|m| match &a.members[m] {
                    Member::Text { text, .. } => text.value.as_str(),
                    _ => "",
                })
            })
            .collect();
        assert_eq!(members, [Some("void start()"), None]);
    }

    #[test]
    fn classes_members_and_lines() {
        let diagram = diagram(
            "@startuml\nclass Product {\n  - id : int\n  {static} + count : int\n  --\n  + name() : String\n}\ninterface Sellable\nProduct <|-- Book\nSellable <|.. Book\nclass Shelf\nShelf *-- \"1\" Product : holds\n@enduml\n",
        );
        let product = element(&diagram.root, "Product");
        assert!(matches!(
            product.members[0],
            Member::Text {
                underline: false,
                slot: Slot::Attribute,
                ..
            }
        ));
        assert!(matches!(
            product.members[1],
            Member::Text {
                underline: true,
                ..
            }
        ));
        assert!(matches!(product.members[2], Member::Rule(_)));
        assert!(matches!(
            product.members[3],
            Member::Text {
                slot: Slot::Method,
                ..
            }
        ));
        assert_eq!(element(&diagram.root, "Sellable").shape, Shape::Interface);
        let extends = diagram
            .root
            .edges
            .iter()
            .find(|edge| edge.from_head == Head::Triangle && edge.dash == Dash::Solid)
            .unwrap();
        assert_eq!(diagram.root.nodes[extends.from].key, "Product");
        assert_eq!(diagram.root.nodes[extends.to].key, "Book");
        let owns = diagram
            .root
            .edges
            .iter()
            .find(|edge| edge.from_head == Head::DiamondFilled)
            .unwrap();
        assert_eq!(
            owns.to_card.as_ref().map(|text| text.value.as_str()),
            Some("1")
        );
        assert_eq!(
            owns.label.as_ref().map(|text| text.value.as_str()),
            Some("holds")
        );
    }

    #[test]
    fn a_package_keeps_its_classes_and_a_line_can_leave_it() {
        let diagram = diagram(
            "@startuml\npackage shop {\n  class Book\n}\nclass Shelf\nShelf --> Book\n@enduml\n",
        );
        let package = element(&diagram.root, "shop");
        assert!(package.composite);
        assert_eq!(package.children.nodes[0].key, "Book");
        assert_eq!(diagram.root.crossings.len(), 1);
        assert_eq!(diagram.root.crossings[0].to, vec![0, 0]);
    }

    #[test]
    fn aliases_colors_and_a_note() {
        let diagram = diagram(
            "@startuml\nclass \"Long name\" as L #pink\nusecase (Login) as UC\nactor :Clerk: as C\n[Web] as W\nnote right of L : here\n@enduml\n",
        );
        let long = element(&diagram.root, "L");
        assert_eq!(long.label.value, "Long name");
        assert_eq!(long.fill.as_deref(), Some("pink"));
        assert_eq!(element(&diagram.root, "UC").shape, Shape::UseCase);
        assert_eq!(element(&diagram.root, "UC").label.value, "Login");
        assert_eq!(element(&diagram.root, "C").shape, Shape::Actor);
        assert_eq!(element(&diagram.root, "W").shape, Shape::Component);
        assert_eq!(diagram.root.notes[0].side, Side::Right);
        assert_eq!(diagram.root.notes[0].target, 0);
    }

    /// A line's own style as PlantUML reads it: its width in its bracket
    /// (`[#red,dashed,thickness=4]`), its color, dash, boldness and words'
    /// color after its far end; a box's border `##[dashed]green` and
    /// `#line.dashed:blue`, six digits without their `#` a color too.
    #[test]
    fn lines_and_borders_take_styles_of_their_own() {
        let d = diagram(
            "@startuml\nclass A\nA -[#red,dashed,thickness=4]-> B\nA --> C #green;line.dotted;text:green : c\n@enduml\n",
        );
        let (b, c) = (&d.root.edges[0], &d.root.edges[1]);
        assert_eq!(
            (b.color.as_deref(), b.dash, b.style.thickness),
            (Some("#red"), Dash::Dashed, Some(4.0))
        );
        assert_eq!(
            (c.color.as_deref(), c.dash, c.style.text.as_deref()),
            (Some("green"), Dash::Dotted, Some("green"))
        );
        assert_eq!(c.label.as_ref().unwrap().value, "c");
        let d = diagram(
            "@startuml\nclass A #pink ##[dashed]green\nclass B #line.dotted:blue\nclass F #back:red;line:00FFFF\n@enduml\n",
        );
        let [a, b, f] = [&d.root.nodes[0], &d.root.nodes[1], &d.root.nodes[2]];
        assert_eq!(
            (a.line_color.as_deref(), a.border_dash.is_some()),
            (Some("green"), true)
        );
        assert_eq!(
            (b.line_color.as_deref(), b.border_dash.is_some()),
            (Some("blue"), true)
        );
        assert_eq!(f.line_color.as_deref(), Some("00FFFF"));
        // Drawn: the line that wide, its head grown with it, its words in
        // their color.
        let svg = crate::render("@startuml\nclass A\nA -[thickness=8]-> B : wide\nA --> C #line:red;text:red : red\n@enduml\n").unwrap();
        assert!(svg.contains("stroke-width=\"8\""), "{svg}");
        let red = &svg[..svg.find(">red<").unwrap()];
        assert!(
            red[red.rfind("<tspan").unwrap()..].contains("fill=\"#FF0000\""),
            "{svg}"
        );
        assert!(
            super::super::draw::head_spans_grow(),
            "a wide line's head grows"
        );
    }

    #[test]
    fn unknown_lines_name_themselves() {
        let err = declined("@startuml\nclass Foo\ntogether {\nclass Bar\n@enduml\n");
        assert_eq!(err.line, Some(2));
        assert!(err.reason.contains("together"), "{}", err.reason);
        let err = declined("@startuml\nclass A\nA -[hidden,bold]-> B\n@enduml\n");
        assert!(err.reason.contains("hidden"), "{}", err.reason);
        let err = declined("@startuml\ncomponent C {\n  port p\n}\n@enduml\n");
        assert!(err.reason.contains("port"), "{}", err.reason);
        let err = declined("@startuml\nmap Data {\n}\n@enduml\n");
        assert!(err.reason.contains("a `map`"), "{}", err.reason);
        let err = declined("@startuml\nclass A\njson J {\n}\n@enduml\n");
        assert!(err.reason.contains("a `json` element"), "{}", err.reason);
        // What stands beside a name and is no cardinality says what it is.
        let err = declined("@startuml\nA }|..|| B\n@enduml\n");
        assert!(err.reason.contains("line end"), "{}", err.reason);
        let err = declined("@startuml\nactor foo\nfoo #line:red --> (bar)\n@enduml\n");
        assert!(err.reason.contains("own style"), "{}", err.reason);
        // After the far end it is the line's own style.
        let d = diagram(
            "@startuml\nactor foo\nfoo --> (bar) #line:red;line.bold;text:blue : l\n@enduml\n",
        );
        let edge = &d.root.edges[0];
        assert_eq!(
            (edge.color.as_deref(), edge.bold, edge.style.text.as_deref()),
            (Some("red"), true, Some("blue"))
        );
        let err = declined("@startuml\n-class \"Private\" {\n}\n@enduml\n");
        assert!(err.reason.contains("visibility"), "{}", err.reason);
    }

    /// REQ-060: `() "Clipboard"` is an interface a component diagram draws
    /// as a circle, at a line's end and declared alone; an `interface`
    /// without members beside components is one too, as PlantUML draws it.
    /// Without components, or with members, an interface stays a class
    /// diagram's box.
    #[test]
    fn an_interface_beside_components_is_a_lollipop() {
        let d = diagram(
            "@startuml\n[Renderer] ..> () \"Clipboard\"\n() \"Files\" as F\n() HTTP\n[Renderer] --> F\ninterface Plugin\ninterface Port {\n  +open()\n}\n() Cache --> [Renderer]\n@enduml\n",
        );
        for name in ["Clipboard", "F", "HTTP", "Plugin", "Cache"] {
            let node = element(&d.root, name);
            assert_eq!(node.shape, Shape::Lollipop, "{name}");
            assert!(node.stereotypes.is_empty(), "{name}");
        }
        assert_eq!(element(&d.root, "F").label.value, "Files");
        assert_eq!(element(&d.root, "Port").shape, Shape::Interface);
        let dashed = d
            .root
            .edges
            .iter()
            .find(|e| d.root.nodes[e.to].key == "Clipboard")
            .expect("the line to the clipboard");
        assert_eq!((dashed.dash, dashed.to_head), (Dash::Dashed, Head::Arrow));
        // Declared once as an interface and once as a circle, it is one
        // element, a circle; `{` after a circle opens nothing.
        for source in [
            "@startuml\ninterface F #FF0000\n() \"Files\" as F\n@enduml\n",
            "@startuml\n() Files\ninterface Files\n@enduml\n",
            "@startuml\ninterface F\n[A] --> () F\n@enduml\n",
        ] {
            let d = diagram(source);
            assert_eq!(
                d.root
                    .nodes
                    .iter()
                    .filter(|n| n.shape == Shape::Lollipop)
                    .count(),
                1,
                "{source}"
            );
        }
        assert!(
            parse(&lines_of(
                "@startuml\n() Port\n{\n}\n[A] --> Port\n@enduml\n"
            ))
            .is_err()
        );
        // A class diagram's interface keeps its box and its «interface».
        let d = diagram("@startuml\ninterface Shape\nclass Circle\nShape <|.. Circle\n@enduml\n");
        assert_eq!(element(&d.root, "Shape").shape, Shape::Interface);
        assert!(crate::render("@startuml\n[Renderer] ..> () \"Clipboard\"\n@enduml\n").is_ok());
    }

    #[test]
    fn a_direction_inside_a_package_holds_for_the_whole_diagram() {
        let diagram = diagram(
            "@startuml\nA --> B\npackage P {\n  left to right direction\n  C --> D\n}\n@enduml\n",
        );
        assert_eq!(diagram.root.direction, Direction::Right);
        assert_eq!(
            element(&diagram.root, "P").children.direction,
            Direction::Right
        );
    }

    #[test]
    fn extends_and_a_direction() {
        let diagram = diagram(
            "@startuml\nleft to right direction\nabstract class Child extends Parent implements Side\n@enduml\n",
        );
        assert_eq!(diagram.root.direction, Direction::Right);
        assert_eq!(element(&diagram.root, "Child").shape, Shape::Abstract);
        assert_eq!(diagram.root.edges.len(), 2);
        assert!(
            diagram
                .root
                .edges
                .iter()
                .any(|edge| edge.dash == Dash::Dashed)
        );
        assert_eq!(element(&diagram.root, "Side").shape, Shape::Interface);
    }
}
