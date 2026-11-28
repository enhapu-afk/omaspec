//! `hide`, `show`, `remove` and `restore`: which boxes a structure
//! diagram shows, and what of each. PlantUML reads these lines wherever
//! they stand and applies them once every box is known, in the order
//! written: of the lines that name a box, the last one decides.
//!
//! A box hidden keeps its place and is not drawn, nor are its lines; a
//! box removed is gone, with its lines and its notes. `hide circle`
//! takes away the circle before a class's name; the look here draws no
//! empty compartment, so `hide empty members` changes nothing.

use std::collections::{HashMap, HashSet};

use super::{Element, Group, Member, Shape, Slot, SpotKind};
use crate::Declined;
use crate::common::{keyword, snippet};

/// One of these lines, as read.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Directive {
    line: usize,
    what: What,
}

#[derive(Debug, Clone, PartialEq)]
enum What {
    /// `hide members`, `show Foo methods`, `hide private fields`, `hide
    /// <<Entity>> circle`: a part of the boxes `target` names. Members
    /// of the marks in `visibility` only, when it names any.
    Part {
        show: bool,
        target: Target,
        part: Part,
        visibility: Vec<char>,
    },
    /// `hide empty members`: a compartment without members, which is
    /// never drawn here.
    Nothing,
    /// `hide Foo`, `show <<Entity>>`, `hide @unlinked`.
    Seen { show: bool, target: Target },
    /// `remove Foo`, `restore $tag`.
    Kept { keep: bool, target: Target },
}

#[derive(Debug, Clone, PartialEq)]
enum Target {
    /// No target, or `*`.
    All,
    /// `class`, `interface`, `struct`, …: every box of that kind.
    Kind(Kind),
    /// `<<Entity>>`: every box with that stereotype.
    Stereotype(String),
    /// A box by its name; a kind's word when no box has that name.
    Name(String),
    /// `$tag`: every box declared with it.
    Tag(String),
    /// `@unlinked`: every box no line meets.
    Unlinked,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Part {
    Members,
    Fields,
    Methods,
    Circle,
    Stereotype,
}

const VERBS: [&str; 4] = ["hide", "show", "remove", "restore"];

/// The verb `text` starts with, when it is one of these lines.
pub(super) fn verb(text: &str) -> Option<&'static str> {
    VERBS.into_iter().find(|verb| keyword(text, verb).is_some())
}

/// Whether `text` hides or shows a part only a class diagram has, so the
/// diagram is one: PlantUML's sequence diagram takes no such line.
pub(super) fn marks_structure(text: &str) -> bool {
    let Some(rest) = keyword(text, "hide").or_else(|| keyword(text, "show")) else {
        return false;
    };
    tokens(rest).is_some_and(|tokens| tokens.last().and_then(|last| part_of(last)).is_some())
}

/// The line `text` as one of these, or `None` when it is no such line.
pub(super) fn directive(text: &str, line: usize) -> Result<Option<Directive>, Declined> {
    let Some(verb) = verb(text) else {
        return Ok(None);
    };
    let rest = keyword(text, verb).unwrap_or_default();
    let unread = || Declined::at(line, format!("`{}` is not supported yet", snippet(text)));
    let tokens = tokens(rest).ok_or_else(unread)?;
    if tokens.is_empty() {
        return Err(Declined::at(
            line,
            format!("`{verb}` needs what it {verb}s"),
        ));
    }
    let what = match verb {
        "hide" | "show" => {
            let show = verb == "show";
            match tokens.split_last() {
                Some((last, before)) if part_of(last).is_some() => {
                    let part = part_of(last).expect("a part");
                    part_line(show, before, part).ok_or_else(unread)?
                }
                _ if tokens.len() == 1 => What::Seen {
                    show,
                    target: target_of(&tokens[0]).ok_or_else(unread)?,
                },
                _ => return Err(unread()),
            }
        }
        _ => {
            let [one] = tokens.as_slice() else {
                return Err(unread());
            };
            What::Kept {
                keep: verb == "restore",
                target: target_of(one).ok_or_else(unread)?,
            }
        }
    };
    Ok(Some(Directive { line, what }))
}

/// The words of a line, a stereotype (`<<a b>>`) and a quoted name each
/// as one, with its marks; commas part words like spaces.
fn tokens(text: &str) -> Option<Vec<String>> {
    let parted = |c: char| c.is_whitespace() || c == ',';
    let mut out = Vec::new();
    let mut rest = text.trim_start_matches(parted);
    while !rest.is_empty() {
        let (token, after) = if let Some(inner) = rest.strip_prefix("<<") {
            let end = inner.find(">>")?;
            (format!("<<{}>>", &inner[..end]), &inner[end + 2..])
        } else if let Some(inner) = rest.strip_prefix('"') {
            let end = inner.find('"')?;
            (format!("\"{}\"", &inner[..end]), &inner[end + 1..])
        } else {
            let end = rest.find(parted).unwrap_or(rest.len());
            (rest[..end].to_owned(), &rest[end..])
        };
        out.push(token);
        rest = after.trim_start_matches(parted);
    }
    Some(out)
}

/// The part a word names: `members`, `fields`, `circle`, ….
fn part_of(word: &str) -> Option<Part> {
    let word = word.to_ascii_lowercase();
    Some(match word.as_str() {
        "member" | "members" => Part::Members,
        "field" | "fields" | "attribute" | "attributes" => Part::Fields,
        "method" | "methods" => Part::Methods,
        "stereotype" | "stereotypes" => Part::Stereotype,
        _ if word.starts_with("circle") => Part::Circle,
        _ => return None,
    })
}

/// `hide [target] [empty] part` or `hide private, protected members`.
fn part_line(show: bool, before: &[String], part: Part) -> Option<What> {
    let mut empty = false;
    let mut visibility = Vec::new();
    let mut target = None;
    for token in before {
        let mark = match token.to_ascii_lowercase().as_str() {
            "empty" => {
                empty = true;
                continue;
            }
            "public" => '+',
            "private" => '-',
            "protected" => '#',
            "package" => '~',
            _ => {
                if target.is_some() {
                    return None;
                }
                target = Some(target_of(token)?);
                continue;
            }
        };
        visibility.push(mark);
    }
    let members = matches!(part, Part::Members | Part::Fields | Part::Methods);
    if empty {
        return members.then_some(What::Nothing);
    }
    if !visibility.is_empty() && !members {
        return None;
    }
    Some(What::Part {
        show,
        target: target.unwrap_or(Target::All),
        part,
        visibility,
    })
}

fn target_of(token: &str) -> Option<Target> {
    if token == "*" {
        return Some(Target::All);
    }
    if token.eq_ignore_ascii_case("@unlinked") {
        return Some(Target::Unlinked);
    }
    if let Some(tag) = token.strip_prefix('$') {
        return (!tag.is_empty()).then(|| Target::Tag(tag.to_owned()));
    }
    if let Some(inner) = token.strip_prefix("<<").and_then(|t| t.strip_suffix(">>")) {
        let inner = inner.trim();
        return (!inner.is_empty()).then(|| Target::Stereotype(inner.to_owned()));
    }
    if let Some(inner) = token.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        return (!inner.is_empty()).then(|| Target::Name(inner.to_owned()));
    }
    (!token.starts_with(['@', '<', '"'])).then(|| Target::Name(token.to_owned()))
}

/// The boxes a kind's word names, as PlantUML's `hide` takes them.
/// A kind of box a word names: by the kind its circle shows, where it
/// has one (a `struct` is no `class`, a `protocol` no `interface`), else
/// by its shape.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Kind {
    spot: Option<SpotKind>,
    shapes: &'static [Shape],
}

impl Kind {
    fn holds(self, el: &Element) -> bool {
        match (&el.spot, self.spot) {
            (Some(spot), Some(kind)) => spot.kind == kind,
            (Some(_), None) => false,
            (None, _) => self.shapes.contains(&el.shape),
        }
    }
}

fn kind_of(word: &str) -> Option<Kind> {
    let kind = |spot, shapes| Some(Kind { spot, shapes });
    match word.to_ascii_lowercase().as_str() {
        "class" => kind(Some(SpotKind::Class), &[Shape::Class]),
        "abstract" => kind(Some(SpotKind::Abstract), &[Shape::Abstract]),
        "interface" => kind(
            Some(SpotKind::Interface),
            &[Shape::Interface, Shape::Lollipop],
        ),
        "enum" => kind(Some(SpotKind::Enum), &[Shape::Enum]),
        "annotation" => kind(Some(SpotKind::Annotation), &[Shape::Annotation]),
        "entity" => kind(Some(SpotKind::Entity), &[Shape::Entity]),
        "protocol" => kind(Some(SpotKind::Protocol), &[]),
        "struct" => kind(Some(SpotKind::Struct), &[]),
        "exception" => kind(Some(SpotKind::Exception), &[]),
        "metaclass" => kind(Some(SpotKind::MetaClass), &[]),
        "record" => kind(Some(SpotKind::Record), &[]),
        "dataclass" => kind(Some(SpotKind::DataClass), &[]),
        "object" => kind(None, &[Shape::Object]),
        _ => None,
    }
}

/// The `$tag`s on a declaration taken out of it: the line without them,
/// and the tags. A tag is a word of its own of letters, digits, `_`, `.`
/// and `-` after a `$`; a `$` in quotes, in a stereotype, in brackets or
/// parentheses (`[Pay $5]`, a link) stays where it is.
pub(super) fn take_tags(text: &str) -> (String, Vec<String>) {
    let tag_char = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '-');
    let mut kept = String::with_capacity(text.len());
    let mut tags = Vec::new();
    let mut rest = text;
    let mut quoted = false;
    // How deep in `<<`, `[` and `(` the next character is.
    let mut depth = 0usize;
    while let Some(c) = rest.chars().next() {
        let at_word = kept.is_empty() || kept.ends_with(char::is_whitespace);
        if !quoted && depth == 0 && c == '$' && at_word {
            let end = rest[1..]
                .find(|c: char| !tag_char(c))
                .map_or(rest.len(), |at| at + 1);
            let ends_word = rest[end..]
                .chars()
                .next()
                .is_none_or(|c| c.is_whitespace() || c == '{');
            if end > 1 && ends_word {
                tags.push(rest[1..end].to_owned());
                rest = rest[end..].trim_start();
                continue;
            }
        }
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            if rest.starts_with("<<") {
                depth += 1;
                kept.push_str("<<");
                rest = &rest[2..];
                continue;
            }
            if rest.starts_with(">>") && depth > 0 {
                depth -= 1;
                kept.push_str(">>");
                rest = &rest[2..];
                continue;
            }
            match c {
                '[' | '(' => depth += 1,
                ']' | ')' => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
        kept.push(c);
        rest = &rest[c.len_utf8()..];
    }
    (kept.trim_end().to_owned(), tags)
}

/// Lays `directives` on the boxes of `root`; `tags` are each box's
/// `$tag`s, by its path. A name that is no box and no kind's word is
/// declined on its line.
/// `separator` parts a class's whole name into its packages (`a.b.C`),
/// which a directive may name it by.
pub(super) fn apply(
    root: &mut Group,
    directives: &[Directive],
    tags: &[(Vec<usize>, String)],
    separator: Option<&str>,
) -> Result<(), Declined> {
    if directives.is_empty() {
        return Ok(());
    }
    let mut boxes = Vec::new();
    walk(root, &mut Vec::new(), &mut boxes);
    // Each box's whole name, through the packages it stands in, by their
    // names (`mp.C` in `package "My Pkg" as mp`); a box made from a whole
    // name (`a.b.C`) has it already.
    let whole: Vec<Option<String>> = boxes
        .iter()
        .map(|path| {
            let sep = separator?;
            (path.len() > 1).then(|| {
                (1..=path.len()).fold(String::new(), |whole, depth| {
                    let key = &node(root, &path[..depth]).key;
                    if whole.is_empty() || key.starts_with(&format!("{whole}{sep}")) {
                        key.clone()
                    } else {
                        format!("{whole}{sep}{key}")
                    }
                })
            })
        })
        .collect();
    let directives = resolved(root, &boxes, &whole, separator, directives)?;
    let mut linked = HashSet::new();
    lines_of(root, &mut Vec::new(), &mut linked);
    let whole: HashMap<Vec<usize>, String> = boxes
        .iter()
        .cloned()
        .zip(whole)
        .filter_map(|(path, whole)| Some((path, whole?)))
        .collect();
    let world = World {
        linked: &linked,
        tags,
        directives: &directives,
        whole: &whole,
        separator,
    };
    let mut gone = HashSet::new();
    let mut unseen = HashSet::new();
    let mut plans = Vec::with_capacity(boxes.len());
    for path in &boxes {
        let el = node(root, path);
        // A point has no name to restore it by: its pair decides it.
        if el.shape != Shape::Point {
            if world.last(path, el, kept) == Some(false) {
                gone.insert(path.clone());
            }
            if world.last(path, el, seen) == Some(false) {
                unseen.insert(path.clone());
            }
        }
        plans.push(world.plan(path, el));
    }
    // An association class's point goes, and is hidden, with either box
    // of its pair, whose line runs through it, and only so.
    for path in &boxes {
        if node(root, path).shape == Shape::Point {
            let pair = pair_of(root, path);
            if pair.iter().any(|end| within(&gone, end)) {
                gone.insert(path.clone());
            }
            if pair.iter().any(|end| within(&unseen, end)) {
                unseen.insert(path.clone());
            }
        }
    }
    for (path, plan) in boxes.iter().zip(plans) {
        plan.lay(node_mut(root, path));
    }
    if !unseen.is_empty() {
        unsee(root, &mut Vec::new(), &unseen, false);
    }
    if !gone.is_empty() {
        prune(root, &mut Vec::new(), &gone);
    }
    Ok(())
}

/// The directives with each name that is no box read as a kind's word.
fn resolved(
    root: &Group,
    boxes: &[Vec<usize>],
    whole: &[Option<String>],
    separator: Option<&str>,
    directives: &[Directive],
) -> Result<Vec<Directive>, Declined> {
    let named = |name: &str| {
        boxes
            .iter()
            .zip(whole)
            .any(|(path, whole)| called(node(root, path), whole.as_deref(), separator, name))
    };
    directives
        .iter()
        .map(|directive| {
            let mut directive = directive.clone();
            let target = match &mut directive.what {
                What::Part { target, .. }
                | What::Seen { target, .. }
                | What::Kept { target, .. } => target,
                What::Nothing => return Ok(directive),
            };
            // The last part of a whole name (`Socket` of `net.Socket`)
            // names no box, and PlantUML passes it by.
            let last_part = |name: &str| {
                boxes.iter().any(|path| {
                    let el = node(root, path);
                    el.label.value == name && el.key != name
                })
            };
            if let Target::Name(name) = target
                && !named(name)
                && !last_part(name)
            {
                *target = Target::Kind(kind_of(name).ok_or_else(|| {
                    Declined::at(directive.line, format!("no box is called `{name}`"))
                })?);
            }
            Ok(directive)
        })
        .collect()
}

/// Whether a box is called `name`: by its key, its whole name through
/// the packages it stands in (`a.b.C`), or its label, but where that is
/// only the last part of the whole name it was written by (a class made
/// from `net.Socket` is not `Socket`), as PlantUML names them.
fn called(el: &Element, whole: Option<&str>, separator: Option<&str>, name: &str) -> bool {
    let made = separator.is_some_and(|sep| {
        el.key != el.label.value && el.key.ends_with(&format!("{sep}{}", el.label.value))
    });
    el.key == name || whole == Some(name) || (el.label.value == name && !made)
}

/// What the directives are read against.
struct World<'a> {
    linked: &'a HashSet<Vec<usize>>,
    tags: &'a [(Vec<usize>, String)],
    directives: &'a [Directive],
    /// Each box's whole name through its packages ([`called`]).
    whole: &'a HashMap<Vec<usize>, String>,
    separator: Option<&'a str>,
}

fn kept(what: &What) -> Option<(bool, &Target)> {
    match what {
        What::Kept { keep, target } => Some((*keep, target)),
        _ => None,
    }
}

fn seen(what: &What) -> Option<(bool, &Target)> {
    match what {
        What::Seen { show, target } => Some((*show, target)),
        _ => None,
    }
}

impl World<'_> {
    fn names(&self, target: &Target, path: &[usize], el: &Element) -> bool {
        match target {
            Target::All => true,
            Target::Kind(kind) => kind.holds(el),
            Target::Stereotype(name) => el.stereotypes.iter().any(|s| s.value == *name),
            Target::Name(name) => called(
                el,
                self.whole.get(path).map(String::as_str),
                self.separator,
                name,
            ),
            Target::Tag(tag) => self
                .tags
                .iter()
                .any(|(at, written)| at == path && written == tag),
            // A note of its own is no box a line is missing from.
            Target::Unlinked => {
                !el.composite && el.shape != Shape::Note && !self.linked.contains(path)
            }
        }
    }

    /// What the last directive `pick` takes that names the box says.
    fn last(
        &self,
        path: &[usize],
        el: &Element,
        pick: fn(&What) -> Option<(bool, &Target)>,
    ) -> Option<bool> {
        self.directives
            .iter()
            .rev()
            .filter_map(|directive| pick(&directive.what))
            .find(|(_, target)| self.names(target, path, el))
            .map(|(show, _)| show)
    }

    /// Whether the last directive about `part` of the box shows it, for a
    /// member of `mark` and `slot` when it is about members.
    fn shows(
        &self,
        path: &[usize],
        el: &Element,
        about: impl Fn(Part) -> bool,
        mark: Option<char>,
    ) -> bool {
        self.directives
            .iter()
            .rev()
            .find_map(|directive| match &directive.what {
                What::Part {
                    show,
                    target,
                    part,
                    visibility,
                } if about(*part)
                    && (visibility.is_empty() || mark.is_some_and(|m| visibility.contains(&m)))
                    && self.names(target, path, el) =>
                {
                    Some(*show)
                }
                _ => None,
            })
            .unwrap_or(true)
    }

    fn plan(&self, path: &[usize], el: &Element) -> Plan {
        let members = el
            .members
            .iter()
            .map(|member| match member {
                Member::Text { text, slot, .. } => {
                    let mark = text
                        .value
                        .chars()
                        .next()
                        .filter(|c| matches!(c, '+' | '-' | '#' | '~'));
                    let slot = *slot;
                    self.shows(
                        path,
                        el,
                        |part| match part {
                            Part::Members => true,
                            Part::Fields => slot == Slot::Attribute,
                            Part::Methods => slot == Slot::Method,
                            Part::Circle | Part::Stereotype => false,
                        },
                        mark,
                    )
                }
                Member::Rule(_) | Member::Heading(_) => true,
            })
            .collect();
        Plan {
            members,
            circle: self.shows(path, el, |part| part == Part::Circle, None),
            stereotype: self.shows(path, el, |part| part == Part::Stereotype, None),
        }
    }
}

/// What of one box stays.
struct Plan {
    /// Each member, by its place.
    members: Vec<bool>,
    circle: bool,
    stereotype: bool,
}

impl Plan {
    fn lay(self, el: &mut Element) {
        if self.members.iter().any(|shown| !shown) {
            let mut shown = self.members.into_iter();
            el.members.retain(|_| shown.next().unwrap_or(true));
            tidy(&mut el.members);
        }
        if !self.stereotype {
            el.stereotypes.clear();
        }
        if !self.circle {
            el.spot = None;
        }
    }
}

/// The rules and headings left around the members that stay: none when
/// no member stays, none first, last or twice in a row, and no heading
/// over nothing.
fn tidy(members: &mut Vec<Member>) {
    let texts = |from: &[Member]| {
        from.iter()
            .take_while(|m| !matches!(m, Member::Heading(_)))
            .any(|m| matches!(m, Member::Text { .. }))
    };
    let all = std::mem::take(members);
    for (at, member) in all.iter().enumerate() {
        let keep = match member {
            Member::Text { .. } => true,
            Member::Heading(_) => texts(&all[at + 1..]),
            Member::Rule(_) => {
                !matches!(members.last(), None | Some(Member::Rule(_)))
                    && all[at + 1..]
                        .iter()
                        .any(|m| matches!(m, Member::Text { .. }))
            }
        };
        if keep {
            members.push(member.clone());
        }
    }
}

/// Every box under `group`, by its path, a container before what it holds.
fn walk(group: &Group, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
    for (index, node) in group.nodes.iter().enumerate() {
        path.push(index);
        out.push(path.clone());
        walk(&node.children, path, out);
        path.pop();
    }
}

/// The boxes a line meets, by their paths.
fn lines_of(group: &Group, path: &mut Vec<usize>, out: &mut HashSet<Vec<usize>>) {
    let at = |end: &[usize]| [path.as_slice(), end].concat();
    for edge in &group.edges {
        out.insert(at(&[edge.from]));
        out.insert(at(&[edge.to]));
    }
    for crossing in &group.crossings {
        out.insert(at(&crossing.from));
        out.insert(at(&crossing.to));
    }
    // A hidden line across the flow holds its box beside the other.
    for held in &group.held {
        out.insert(at(&[held.index]));
        out.insert(at(&[held.beside]));
    }
    for (index, node) in group.nodes.iter().enumerate() {
        path.push(index);
        lines_of(&node.children, path, out);
        path.pop();
    }
}

/// The two boxes of the pair whose line runs through the point at
/// `path`, by their paths: those its key names, `(A, B)`, among the boxes
/// its lines meet.
fn pair_of(root: &Group, path: &[usize]) -> Vec<Vec<usize>> {
    let (&index, home) = path.split_last().expect("a box has a path");
    let mut group = root;
    for &at in home {
        group = &group.nodes[at].children;
    }
    let key = &group.nodes[index].key;
    let ends: Vec<usize> = group
        .edges
        .iter()
        .filter_map(|edge| match (edge.from == index, edge.to == index) {
            (true, false) => Some(edge.to),
            (false, true) => Some(edge.from),
            _ => None,
        })
        .collect();
    let named =
        |a: usize, b: usize| *key == format!("({}, {})", group.nodes[a].key, group.nodes[b].key);
    ends.iter()
        .flat_map(|&a| ends.iter().map(move |&b| (a, b)))
        .find(|&(a, b)| named(a, b))
        .map(|(a, b)| vec![[home, &[a]].concat(), [home, &[b]].concat()])
        .unwrap_or_default()
}

fn node<'a>(root: &'a Group, path: &[usize]) -> &'a Element {
    let (last, home) = path.split_last().expect("a box has a path");
    let mut group = root;
    for &index in home {
        group = &group.nodes[index].children;
    }
    &group.nodes[*last]
}

fn node_mut<'a>(root: &'a mut Group, path: &[usize]) -> &'a mut Element {
    let (last, home) = path.split_last().expect("a box has a path");
    let mut group = root;
    for &index in home {
        group = &mut group.nodes[index].children;
    }
    &mut group.nodes[*last]
}

/// Whether `path`, or a container around it, is in `set`.
fn within(set: &HashSet<Vec<usize>>, path: &[usize]) -> bool {
    (1..=path.len()).any(|len| set.contains(&path[..len]))
}

/// Hides the boxes of `unseen` and what they hold, and the lines that
/// meet them; `around` when a container around `group` is hidden. A note
/// on a hidden box keeps its place and is not drawn either.
fn unsee(group: &mut Group, path: &mut Vec<usize>, unseen: &HashSet<Vec<usize>>, around: bool) {
    let hidden = |end: &[usize], path: &[usize]| around || within(unseen, &[path, end].concat());
    for edge in &mut group.edges {
        if hidden(&[edge.from], path) || hidden(&[edge.to], path) {
            edge.hidden = true;
        }
    }
    for crossing in &mut group.crossings {
        if hidden(&crossing.from, path) || hidden(&crossing.to, path) {
            crossing.arrow.hidden = true;
        }
    }
    for (index, node) in group.nodes.iter_mut().enumerate() {
        path.push(index);
        let gone = around || unseen.contains(path.as_slice());
        node.hidden |= gone;
        unsee(&mut node.children, path, unseen, gone);
        path.pop();
    }
}

/// `path` once the boxes of `gone` are taken out, or `None` when it is
/// one of them or in one.
fn renumbered(gone: &HashSet<Vec<usize>>, path: &[usize]) -> Option<Vec<usize>> {
    let mut out = Vec::with_capacity(path.len());
    for (depth, &index) in path.iter().enumerate() {
        if gone.contains(&path[..=depth]) {
            return None;
        }
        let mut sibling = path[..=depth].to_vec();
        let before = (0..index)
            .filter(|&other| {
                sibling[depth] = other;
                gone.contains(&sibling)
            })
            .count();
        out.push(index - before);
    }
    Some(out)
}

/// Takes the boxes of `gone` out of `group`, at `path`, with their lines
/// and notes; every index left points where it pointed.
fn prune(group: &mut Group, path: &mut Vec<usize>, gone: &HashSet<Vec<usize>>) {
    let depth = path.len();
    let moved = |end: &[usize], path: &[usize]| {
        renumbered(gone, &[path, end].concat()).map(|new| new[depth..].to_vec())
    };
    group.edges.retain_mut(
        |edge| match (moved(&[edge.from], path), moved(&[edge.to], path)) {
            (Some(from), Some(to)) => {
                (edge.from, edge.to) = (from[0], to[0]);
                true
            }
            _ => false,
        },
    );
    group.crossings.retain_mut(|crossing| {
        match (moved(&crossing.from, path), moved(&crossing.to, path)) {
            (Some(from), Some(to)) => {
                (crossing.from, crossing.to) = (from, to);
                true
            }
            _ => false,
        }
    });
    group
        .notes
        .retain_mut(|note| match moved(&[note.target], path) {
            Some(target) => {
                note.target = target[0];
                true
            }
            None => false,
        });
    // A box held beside another by a hidden line across the flow.
    group.held.retain_mut(
        |held| match (moved(&[held.index], path), moved(&[held.beside], path)) {
            (Some(index), Some(beside)) => {
                (held.index, held.beside) = (index[0], beside[0]);
                true
            }
            _ => false,
        },
    );
    group
        .apart
        .retain_mut(|member| match moved(&[*member], path) {
            Some(new) => {
                *member = new[0];
                true
            }
            None => false,
        });
    for (index, node) in group.nodes.iter_mut().enumerate() {
        path.push(index);
        prune(&mut node.children, path, gone);
        path.pop();
    }
    let mut index = 0;
    group.nodes.retain(|_| {
        path.push(index);
        let keep = !gone.contains(path.as_slice());
        path.pop();
        index += 1;
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::text_at;

    #[test]
    fn a_line_reads_its_target_and_part() {
        let read = |text: &str| directive(text, 0).unwrap().unwrap().what;
        assert_eq!(read("hide empty members"), What::Nothing);
        assert_eq!(
            read("hide circle"),
            What::Part {
                show: false,
                target: Target::All,
                part: Part::Circle,
                visibility: Vec::new(),
            }
        );
        assert_eq!(
            read("show <<Serializable Thing>> fields"),
            What::Part {
                show: true,
                target: Target::Stereotype("Serializable Thing".into()),
                part: Part::Fields,
                visibility: Vec::new(),
            }
        );
        assert_eq!(
            read("hide private, protected methods"),
            What::Part {
                show: false,
                target: Target::All,
                part: Part::Methods,
                visibility: vec!['-', '#'],
            }
        );
        assert_eq!(
            read("remove @unlinked"),
            What::Kept {
                keep: false,
                target: Target::Unlinked,
            }
        );
        assert_eq!(
            read("restore $tag1"),
            What::Kept {
                keep: true,
                target: Target::Tag("tag1".into()),
            }
        );
        assert_eq!(
            read("hide \"Long name\""),
            What::Seen {
                show: false,
                target: Target::Name("Long name".into()),
            }
        );
        for text in [
            "hide",
            "hide A B",
            "remove A methods",
            "hide private circle",
        ] {
            assert!(directive(text, 0).is_err(), "{text}");
        }
        assert_eq!(directive("hidden A", 0).unwrap(), None);
    }

    #[test]
    fn tags_leave_a_declaration_and_quotes_keep_theirs() {
        assert_eq!(
            take_tags("class C1 $tag13 {"),
            ("class C1 {".into(), vec!["tag13".into()])
        );
        assert_eq!(
            take_tags("class \"Costs $5\" as C <<$x>> $a $b"),
            (
                "class \"Costs $5\" as C <<$x>>".into(),
                vec!["a".into(), "b".into()]
            )
        );
        assert_eq!(take_tags("class Plain"), ("class Plain".into(), Vec::new()));
        // A `$` in brackets, parentheses or a link is part of the name.
        for text in [
            "component [Pay $5]",
            "usecase (Pay $5)",
            "class A [[http://x.org/ $y]]",
        ] {
            assert_eq!(take_tags(text), (text.into(), Vec::new()), "{text}");
        }
        assert_eq!(
            take_tags("component [Pay $5] as P $billing"),
            ("component [Pay $5] as P".into(), vec!["billing".into()])
        );
    }

    #[test]
    fn rules_and_headings_go_with_the_members_around_them() {
        let text = |value: &str| Member::Text {
            text: text_at(value, 0),
            italic: false,
            underline: false,
            slot: Slot::Attribute,
        };
        let heading = || Member::Heading(text_at("Head", 0));
        let rule = || Member::Rule(super::super::Dash::Solid);
        // None first, none last, none twice; the one under the heading
        // was written there.
        let mut members = vec![
            rule(),
            heading(),
            rule(),
            text("a"),
            rule(),
            rule(),
            heading(),
        ];
        tidy(&mut members);
        assert_eq!(members, vec![heading(), rule(), text("a")]);
        let mut members = vec![text("a"), rule(), rule(), text("b"), rule()];
        tidy(&mut members);
        assert_eq!(members, vec![text("a"), rule(), text("b")]);
    }

    #[test]
    fn a_path_moves_up_past_the_boxes_taken_out() {
        let gone: HashSet<Vec<usize>> = [vec![0], vec![2, 1]].into_iter().collect();
        assert_eq!(renumbered(&gone, &[1]), Some(vec![0]));
        assert_eq!(renumbered(&gone, &[2, 3]), Some(vec![1, 2]));
        assert_eq!(renumbered(&gone, &[2, 1, 0]), None);
        assert_eq!(renumbered(&gone, &[0, 5]), None);
    }

    fn diagram(body: &str) -> super::super::Diagram {
        let source = format!("@startuml\n{body}\n@enduml\n");
        let (_, lines) = crate::source::body(&source).unwrap();
        super::super::parse(&lines).unwrap_or_else(|err| panic!("{err:?}"))
    }

    fn keys(group: &Group) -> Vec<&str> {
        group.nodes.iter().map(|node| node.key.as_str()).collect()
    }

    #[test]
    fn what_a_class_diagram_hides_and_removes() {
        let d = diagram(
            "class A {\n  -a : int\n  +b()\n}\ninterface I\nclass B <<S>>\nclass C $t\n\
             A --> B\nA ..|> I\nhide private members\nhide circle\nhide <<S>> stereotype\n\
             hide B\nremove $t",
        );
        assert_eq!(keys(&d.root), ["A", "I", "B"]);
        let [a, i, b] = &d.root.nodes[..] else {
            unreachable!()
        };
        assert!(
            matches!(&a.members[..], [Member::Text { text, .. }] if text.value == "+b()"),
            "{:?}",
            a.members
        );
        // The kind's word stands for the circle.
        assert!(i.stereotypes.is_empty(), "{:?}", i.stereotypes);
        assert!(b.hidden && b.stereotypes.is_empty() && !a.hidden);
        // A line into a hidden box is laid out, not drawn.
        let into = |to: &str| {
            d.root
                .edges
                .iter()
                .find(|edge| d.root.nodes[edge.to].key == to)
                .expect("the line")
        };
        assert!(into("B").hidden && !into("I").hidden);
    }

    #[test]
    fn the_last_line_about_a_box_decides() {
        let d = diagram(
            "class A {\n  x\n  +m()\n}\nclass B {\n  y\n  +n()\n}\n\
             hide members\nshow A methods\nremove A\nrestore A",
        );
        let [a, b] = &d.root.nodes[..] else {
            unreachable!()
        };
        assert!(matches!(
            &a.members[..],
            [Member::Text {
                slot: Slot::Method,
                ..
            }]
        ));
        assert!(b.members.is_empty());
        // A stereotype hidden leaves the circle; `hide circle` takes it
        // away, the stereotype staying.
        let d = diagram("interface I <<Remote>>\nhide stereotype");
        let el = &d.root.nodes[0];
        assert!(el.stereotypes.is_empty());
        assert_eq!(el.spot.as_ref().map(|s| s.letter), Some('I'));
        let d = diagram("interface I <<Remote>>\nclass C\nhide I circle");
        assert!(d.root.nodes[0].spot.is_none() && d.root.nodes[0].stereotypes.len() == 1);
        assert!(d.root.nodes[1].spot.is_some());
    }

    #[test]
    fn a_box_removed_takes_its_lines_and_the_rest_keep_their_ends() {
        let d = diagram(
            "package P {\n  class X\n  class Y\n}\nclass Z\nclass W\nX --> Y\nZ --> Y\n\
             note right of X : gone\nnote \"stays\" as N\nremove X\nremove @unlinked",
        );
        assert_eq!(keys(&d.root), ["P", "Z", "N"]);
        let p = &d.root.nodes[0].children;
        assert_eq!(keys(p), ["Y"]);
        assert!(p.edges.is_empty() && p.notes.is_empty());
        let crossing = &d.root.crossings[0];
        assert_eq!(
            (&crossing.from[..], &crossing.to[..]),
            (&[1][..], &[0, 0][..])
        );
    }

    #[test]
    fn a_box_held_beside_another_stays_beside_it_when_one_before_is_removed() {
        let d = diagram("class A\nclass B\nclass C\nclass D\nB -- D\nC -[hidden]left- B\nremove A");
        let [held] = &d.root.held[..] else {
            panic!("{:?}", d.root.held)
        };
        let key = |at: usize| d.root.nodes[at].key.as_str();
        assert_eq!((key(held.index), key(held.beside)), ("B", "C"));
        // A box held so is no box without a line.
        let d = diagram("class B\nclass C\nC -[hidden]left- B\nremove @unlinked");
        assert_eq!(keys(&d.root), ["B", "C"]);
    }

    #[test]
    fn a_tag_belongs_to_the_box_it_is_written_on() {
        // Names belong to their packages: the `A` of `Q` is another box,
        // and keeps no tag of `P`'s, as in PlantUML.
        let d = diagram(
            "package P {\n  class A $t\n  class B\n}\npackage Q {\n  class A\n  class C\n}\nremove $t",
        );
        assert_eq!(keys(&d.root.nodes[0].children), ["B"]);
        assert_eq!(keys(&d.root.nodes[1].children), ["A", "C"]);
        // Where names are the whole diagram's, the second is the same box.
        let d = diagram(
            "set separator none\npackage P {\n  class A $t\n  class B\n}\npackage Q {\n  class A\n  class C\n}\nremove $t",
        );
        assert_eq!(keys(&d.root.nodes[0].children), ["B"]);
        assert_eq!(keys(&d.root.nodes[1].children), ["C"]);
        // A `$` in a name stays in it.
        let d = diagram("component [Pay $5]");
        assert_eq!(d.root.nodes[0].label.value, "Pay $5");
    }

    #[test]
    fn a_hidden_box_keeps_its_note_in_its_place_undrawn() {
        let d = diagram("class A\nclass B\nA --> B\nnote right of B : n\nhide B");
        assert_eq!(d.root.notes.len(), 1);
        let shown =
            crate::render("@startuml\nclass A\nclass B\nA --> B\nnote right of B : n\n@enduml\n")
                .unwrap();
        let hidden = crate::render(
            "@startuml\nclass A\nclass B\nA --> B\nnote right of B : n\nhide B\n@enduml\n",
        )
        .unwrap();
        assert!(
            shown.contains(">n</tspan>") && !hidden.contains(">n</tspan>"),
            "{hidden}"
        );
        // A stands where it stood.
        let first_rect = |svg: &str| {
            svg[svg.find("<rect").unwrap()..]
                .split('>')
                .next()
                .unwrap()
                .to_owned()
        };
        assert_eq!(first_rect(&shown), first_rect(&hidden));
    }

    #[test]
    fn a_name_that_is_no_box_is_said() {
        for (body, word) in [
            ("class A\nhide Nobody", "Nobody"),
            ("class A\nhide footbox", "footbox"),
        ] {
            let source = format!("@startuml\n{body}\n@enduml\n");
            let (_, lines) = crate::source::body(&source).unwrap();
            let err = super::super::parse(&lines).unwrap_err();
            assert_eq!(err.line, Some(2), "{err:?}");
            assert!(err.reason.contains(word), "{}", err.reason);
        }
        // A kind's word is no box's name: every class.
        let d = diagram("class A\ninterface I\nhide class");
        assert!(d.root.nodes[0].hidden && !d.root.nodes[1].hidden);
    }

    #[test]
    fn lines_a_sequence_diagram_has_no_message_for_make_a_class_diagram() {
        use crate::UmlType;
        let of = |body: &str| crate::uml_type(&format!("@startuml\n{body}\n@enduml\n"));
        for body in [
            "Usuario <|-- Cliente",
            "A *-- B",
            "A o-- B",
            "A -- B",
            "A ..> B",
            "A ..|> B",
            "hide empty members\nA -> B",
        ] {
            assert_eq!(of(body), Some(UmlType::Structure), "{body}");
            crate::render(&format!("@startuml\n{body}\n@enduml\n"))
                .unwrap_or_else(|err| panic!("{body}: {err:?}"));
        }
        for body in ["A -> B", "A --> B : hi", "A ->x B", "A -> B\nhide footbox"] {
            assert_eq!(of(body), Some(UmlType::Sequence), "{body}");
        }
    }
}
