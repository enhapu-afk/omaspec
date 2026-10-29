//! Mermaid `classDiagram` read into the structure tree.
//!
//! The class name is the word before a `~generic~`: Mermaid does not treat
//! the parameter as part of the name, so relations say `Square` for
//! `class Square~Shape~`. A namespace is a package. Its classes are named
//! `Namespace.Class` from the outside and by their short name from
//! anywhere it is unambiguous.
//!
//! A lollipop (`()--`), `direction RL` and `direction BT` are refused:
//! the layout only runs downward or to the right, and a line has no
//! lollipop head. A style may set `fill`, `stroke`, `stroke-width`,
//! `stroke-dasharray` and `color`. Anything else in a style is refused,
//! because a property this drawer drops would be a different picture.
//! `click`, `link` and `callback` do not change the picture.

use std::collections::HashMap;

use super::{
    Crossing, Dash, Diagram, Direction, Element, Group, Head, Link, Member, Note, Shape, Side, Slot,
};
use crate::common::{Frame, text_at};
use crate::graph::across::meeting;

/// The class diagram `source` describes, or `None` when it is not a
/// `classDiagram` or it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a class diagram, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

struct Reason {
    line: usize,
    what: &'static str,
}

enum Stop {
    /// Not a class diagram. The caller leaves it to another drawer.
    Foreign,
    At(Reason),
}

fn read(source: &str) -> Result<Diagram, Stop> {
    let src = scan(source);
    if let Some(line) = src.flat {
        return Err(Stop::At(Reason {
            line,
            what: "flat namespaces",
        }));
    }
    let Some((first, rest)) = src.lines.split_first() else {
        return Err(Stop::Foreign);
    };
    let Some(header) = header_rest(&first.1) else {
        return Err(Stop::Foreign);
    };
    let mut parser = Parser::new(src.title, Direction::Down);
    if !header.is_empty() {
        parser.direction = take_direction(header, first.0).map_err(Stop::At)?;
    }
    for (line, text) in rest {
        parser.line(*line, text).map_err(Stop::At)?;
    }
    parser.finish().map_err(Stop::At)
}

struct Scanned {
    title: Option<crate::common::Text>,
    lines: Vec<(usize, String)>,
    /// `hierarchicalNamespaces: false`, which would flatten packages.
    flat: Option<usize>,
}

fn scan(source: &str) -> Scanned {
    let mut front = false;
    let mut title = None;
    let mut flat = None;
    let mut lines = Vec::new();
    for (no, raw) in source.lines().enumerate() {
        let text = raw.trim();
        if text.is_empty() {
            continue;
        }
        if text == "---" {
            front = !front;
            continue;
        }
        if front {
            if title.is_none()
                && !raw.starts_with([' ', '\t'])
                && let Some(rest) = text.strip_prefix("title:")
            {
                let value = unquote(rest.trim());
                if !value.is_empty() {
                    title = Some(text_at(&value, no));
                }
            }
            let flat_key: String = text.chars().filter(|c| !c.is_whitespace()).collect();
            if flat_key
                .to_ascii_lowercase()
                .starts_with("hierarchicalnamespaces:false")
            {
                flat = Some(no);
            }
            continue;
        }
        if text.starts_with("%%") {
            continue;
        }
        let text = cut_comment(text);
        if !text.is_empty() {
            lines.push((no, text));
        }
    }
    Scanned { title, lines, flat }
}

/// The text after `classDiagram`, or `None` when this line is not that header.
fn header_rest(line: &str) -> Option<&str> {
    const MARK: &str = "classdiagram";
    if line.len() < MARK.len() || !line[..MARK.len()].eq_ignore_ascii_case(MARK) {
        return None;
    }
    let mut rest = &line[MARK.len()..];
    if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case("-v2") {
        rest = &rest[3..];
    }
    if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(rest.trim())
}

fn take_direction(rest: &str, line: usize) -> Result<Direction, Reason> {
    let mut parts = rest.split_whitespace();
    let Some(token) = parts.next() else {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    };
    if parts.next().is_some() {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    }
    direction_of(token, line)
}

fn direction_of(token: &str, line: usize) -> Result<Direction, Reason> {
    match token.to_ascii_lowercase().as_str() {
        "tb" | "td" => Ok(Direction::Down),
        "lr" => Ok(Direction::Right),
        "bt" => Err(Reason {
            line,
            what: "direction BT",
        }),
        "rl" => Err(Reason {
            line,
            what: "direction RL",
        }),
        _ => Err(Reason {
            line,
            what: "an unknown line",
        }),
    }
}

#[derive(Clone, Default)]
struct PaintStyle {
    fill: Option<String>,
    stroke: Option<String>,
    thickness: Option<f32>,
    text: Option<String>,
    dash: Option<(f32, f32)>,
}

struct Rel {
    from: Vec<usize>,
    to: Vec<usize>,
    from_card: Option<String>,
    to_card: Option<String>,
    label: Option<String>,
    from_head: Head,
    to_head: Head,
    dash: Dash,
    line: usize,
}

struct ClassNote {
    name: String,
    context: Vec<String>,
    body: String,
    line: usize,
}

enum Open {
    Ns { path: Vec<String>, line: usize },
    Class { key: String, line: usize },
}

struct Parser {
    root: Group,
    packages: HashMap<String, Vec<usize>>,
    classes: HashMap<String, Vec<usize>>,
    shorts: HashMap<String, Vec<String>>,
    stack: Vec<Open>,
    direction: Direction,
    rels: Vec<Rel>,
    float_notes: Vec<(String, usize)>,
    class_notes: Vec<ClassNote>,
    styles: HashMap<String, PaintStyle>,
    default_style: Option<PaintStyle>,
    attached: HashMap<String, Vec<(String, usize)>>,
    pending_css: Vec<(String, Vec<String>, String, usize)>,
    pending_direct: Vec<(String, Vec<String>, PaintStyle, usize)>,
    title: Option<crate::common::Text>,
    notes: usize,
}

impl Parser {
    fn new(title: Option<crate::common::Text>, direction: Direction) -> Self {
        Self {
            root: Group::default(),
            packages: HashMap::new(),
            classes: HashMap::new(),
            shorts: HashMap::new(),
            stack: Vec::new(),
            direction,
            rels: Vec::new(),
            float_notes: Vec::new(),
            class_notes: Vec::new(),
            styles: HashMap::new(),
            default_style: None,
            attached: HashMap::new(),
            pending_css: Vec::new(),
            pending_direct: Vec::new(),
            title,
            notes: 0,
        }
    }

    fn finish(mut self) -> Result<Diagram, Reason> {
        if let Some(open) = self.stack.last() {
            let line = match open {
                Open::Ns { line, .. } | Open::Class { line, .. } => *line,
            };
            return Err(Reason {
                line,
                what: "an unclosed `{`",
            });
        }
        self.apply_styles()?;
        let rels = std::mem::take(&mut self.rels);
        for rel in rels {
            self.place(&rel)?;
        }
        let notes = std::mem::take(&mut self.class_notes);
        for note in notes {
            let path = self.resolve(&note.name, &note.context, note.line, false)?;
            let index = *path.last().expect("a class has a path");
            let home = path[..path.len() - 1].to_vec();
            self.group_mut(&home).notes.push(Note {
                target: index,
                side: Side::Right,
                text: text_at(&note.body, note.line),
                line: note.line,
            });
        }
        for (text, line) in std::mem::take(&mut self.float_notes) {
            let key = format!("__note{}", self.notes);
            self.notes += 1;
            self.root.nodes.push(note_box(key, text, line));
        }
        let direction = self.direction;
        paint_direction(&mut self.root, direction);
        Ok(Diagram {
            root: self.root,
            frame: Frame {
                title: self.title,
                ..Frame::default()
            },
        })
    }

    fn line(&mut self, line: usize, text: &str) -> Result<(), Reason> {
        let text = text.trim();
        if text == "}" {
            return self.close(line);
        }
        if let Some(key) = self.class_key() {
            return self.body_line(&key, line, text);
        }
        if let Some(rest) = word(text, "classDef") {
            return self.class_def(line, rest);
        }
        if let Some(rest) = word(text, "cssClass") {
            return self.css_class(line, rest);
        }
        if let Some(rest) = word(text, "style") {
            return self.style_line(line, rest);
        }
        if ignored(text) {
            return Ok(());
        }
        if let Some(rest) = word(text, "direction") {
            self.direction = take_direction(rest, line)?;
            return Ok(());
        }
        if let Some(rest) = word(text, "note") {
            return self.note(line, rest);
        }
        if let Some(rest) = word(text, "namespace") {
            return self.namespace(line, rest);
        }
        if let Some(rest) = word(text, "title") {
            if self.title.is_none() && !rest.is_empty() {
                self.title = Some(text_at(rest, line));
            }
            return Ok(());
        }
        if self.relation(line, text)? {
            return Ok(());
        }
        if self.annotation(line, text)? {
            return Ok(());
        }
        if let Some(rest) = word(text, "class") {
            return self.class_decl(line, rest);
        }
        if self.member_statement(line, text)? {
            return Ok(());
        }
        Err(Reason {
            line,
            what: "an unknown line",
        })
    }

    fn close(&mut self, line: usize) -> Result<(), Reason> {
        match self.stack.pop() {
            Some(_) => Ok(()),
            None => Err(Reason {
                line,
                what: "a `}`",
            }),
        }
    }

    fn class_key(&self) -> Option<String> {
        match self.stack.last() {
            Some(Open::Class { key, .. }) => Some(key.clone()),
            _ => None,
        }
    }

    fn innermost_ns(&self) -> Vec<String> {
        self.stack
            .iter()
            .rev()
            .find_map(|open| match open {
                Open::Ns { path, .. } => Some(path.clone()),
                Open::Class { .. } => None,
            })
            .unwrap_or_default()
    }

    fn body_line(&mut self, key: &str, line: usize, text: &str) -> Result<(), Reason> {
        let (text, close) = split_close(text);
        if !text.is_empty() {
            let path = self.classes.get(key).cloned().ok_or(Reason {
                line,
                what: "an unknown class",
            })?;
            self.add_parts(&path, text, line)?;
        }
        if close {
            self.pop_class(line)?;
        }
        Ok(())
    }

    fn pop_class(&mut self, line: usize) -> Result<(), Reason> {
        match self.stack.pop() {
            Some(Open::Class { .. }) => Ok(()),
            Some(other) => {
                self.stack.push(other);
                Err(Reason {
                    line,
                    what: "a `}`",
                })
            }
            None => Err(Reason {
                line,
                what: "a `}`",
            }),
        }
    }

    fn class_decl(&mut self, line: usize, rest: &str) -> Result<(), Reason> {
        let decl = parse_decl(rest, line)?;
        let ns = self.innermost_ns();
        let path = self.ensure_class(&ns, &decl.short, decl.label, &decl.generic, line)?;
        self.add_stereotypes(&path, &decl.stereotypes, line);
        let full = self.full_key(&path).expect("the class was just stored");
        for css in decl.css {
            self.attached
                .entry(full.clone())
                .or_default()
                .push((css, line));
        }
        if let Some(body) = decl.inline {
            self.add_parts(&path, &body, line)?;
        }
        if decl.open {
            self.stack.push(Open::Class { key: full, line });
        }
        Ok(())
    }

    fn annotation(&mut self, line: usize, text: &str) -> Result<bool, Reason> {
        let Some(inner) = text.trim().strip_prefix("<<") else {
            return Ok(false);
        };
        let Some(end) = inner.find(">>") else {
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        };
        let stereo = inner[..end].trim();
        let after = inner[end + 2..].trim();
        if stereo.is_empty() || after.is_empty() {
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        }
        let decl = parse_decl(after, line)?;
        if decl.inline.is_some() || decl.open {
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        }
        let ns = self.innermost_ns();
        let path = self.ensure_class(&ns, &decl.short, decl.label, &decl.generic, line)?;
        let mut stereotypes = vec![stereo.to_owned()];
        stereotypes.extend(decl.stereotypes);
        self.add_stereotypes(&path, &stereotypes, line);
        Ok(true)
    }

    fn member_statement(&mut self, line: usize, text: &str) -> Result<bool, Reason> {
        let (left, member) = split_colon_label(text);
        let Some(member) = member else {
            return Ok(false);
        };
        if left.is_empty() || member.is_empty() || left.chars().any(char::is_whitespace) {
            return Ok(false);
        }
        let name = bare_name(left);
        let ns = self.innermost_ns();
        let path = self.resolve(&name, &ns, line, true)?;
        if let Some(stereo) = stereo_inner(member) {
            self.add_stereotypes(&path, &[stereo], line);
        } else {
            for part in split_semi(member) {
                self.add_member(&path, member_of(part, line)?);
            }
        }
        Ok(true)
    }

    fn relation(&mut self, line: usize, text: &str) -> Result<bool, Reason> {
        let Some((at, token)) = find_rel(text) else {
            return Ok(false);
        };
        if token == "()--" || token == "--()" {
            return Err(Reason {
                line,
                what: "a lollipop",
            });
        }
        let (from_head, to_head, dash) = heads(token).ok_or(Reason {
            line,
            what: "an unknown line",
        })?;
        let (from_name, from_card) = split_left(text[..at].trim()).ok_or(Reason {
            line,
            what: "an unknown line",
        })?;
        let (to_name, to_card, label) =
            split_right(text[at + token.len()..].trim()).ok_or(Reason {
                line,
                what: "an unknown line",
            })?;
        let ns = self.innermost_ns();
        let from = self.resolve(&from_name, &ns, line, true)?;
        let to = self.resolve(&to_name, &ns, line, true)?;
        self.rels.push(Rel {
            from,
            to,
            from_card,
            to_card,
            label,
            from_head,
            to_head,
            dash,
            line,
        });
        Ok(true)
    }

    fn note(&mut self, line: usize, rest: &str) -> Result<(), Reason> {
        let (target, rest) = if let Some(rest) = word(rest, "for") {
            let (name, after) = take_ref(rest).ok_or(Reason {
                line,
                what: "an unknown line",
            })?;
            (Some(name), after)
        } else {
            (None, rest)
        };
        let (body, after) = quoted(rest.trim()).ok_or(Reason {
            line,
            what: "an unknown line",
        })?;
        if !after.is_empty() {
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        }
        if let Some(name) = target {
            self.class_notes.push(ClassNote {
                name,
                context: self.innermost_ns(),
                body,
                line,
            });
        } else if !body.is_empty() {
            self.float_notes.push((body, line));
        }
        Ok(())
    }

    fn namespace(&mut self, line: usize, rest: &str) -> Result<(), Reason> {
        let mut rest = rest.trim().to_owned();
        let mut segments = Vec::new();
        loop {
            let (seg, after) = take_ident(&rest).ok_or(Reason {
                line,
                what: "an unknown line",
            })?;
            segments.push(seg);
            rest = after.trim_start().to_owned();
            if let Some(after) = rest.strip_prefix('.') {
                rest = after.trim_start().to_owned();
                continue;
            }
            break;
        }
        let mut label = None;
        if rest.starts_with('[') {
            let (text, after) = bracket_label(&rest, line)?;
            label = Some(text);
            rest = after;
        }
        let mut full = self.innermost_ns();
        full.extend(segments);
        self.ensure_packages(&full, label, line)?;
        let rest = rest.trim();
        if rest == "{" {
            self.stack.push(Open::Ns { path: full, line });
            return Ok(());
        }
        if let Some(inner) = rest.strip_prefix('{') {
            let inner = inner.trim();
            if inner == "}" {
                return Ok(());
            }
        }
        Err(Reason {
            line,
            what: "an unknown line",
        })
    }

    fn class_def(&mut self, line: usize, rest: &str) -> Result<(), Reason> {
        let mut rest = rest.trim().trim_end_matches(';').trim();
        let mut names = Vec::new();
        let props = loop {
            if rest.is_empty() {
                return Err(Reason {
                    line,
                    what: "a style",
                });
            }
            let token = rest.split_whitespace().next().unwrap_or(rest);
            if token.contains(':') {
                break rest;
            }
            for name in token.trim_end_matches(',').split(',') {
                let name = name.trim().trim_end_matches(';');
                if !name.is_empty() {
                    names.push(name.to_owned());
                }
            }
            rest = rest[token.len()..].trim_start();
        };
        if names.is_empty() {
            return Err(Reason {
                line,
                what: "a style",
            });
        }
        let style = paint_style(props, line)?;
        for name in names {
            if name == "default" {
                self.default_style = Some(style.clone());
            } else {
                self.styles.insert(name, style.clone());
            }
        }
        Ok(())
    }

    fn css_class(&mut self, line: usize, rest: &str) -> Result<(), Reason> {
        let rest = rest.trim().trim_end_matches(';').trim();
        let (ids, after) = if let Some((text, after)) = quoted(rest) {
            (text, after)
        } else {
            take_ref(rest).ok_or(Reason {
                line,
                what: "a style",
            })?
        };
        let style_name = after.split_whitespace().next().ok_or(Reason {
            line,
            what: "a style",
        })?;
        if after.split_whitespace().nth(1).is_some() {
            return Err(Reason {
                line,
                what: "a style",
            });
        }
        let style_name = style_name.trim_end_matches(';').to_owned();
        let context = self.innermost_ns();
        for id in ids.split(',') {
            let id = id.trim();
            if id.is_empty() {
                continue;
            }
            self.pending_css
                .push((bare_name(id), context.clone(), style_name.clone(), line));
        }
        Ok(())
    }

    fn style_line(&mut self, line: usize, rest: &str) -> Result<(), Reason> {
        let (name, props) = take_ref(rest.trim()).ok_or(Reason {
            line,
            what: "a style",
        })?;
        if props.is_empty() {
            return Err(Reason {
                line,
                what: "a style",
            });
        }
        let style = paint_style(props, line)?;
        self.pending_direct
            .push((name, self.innermost_ns(), style, line));
        Ok(())
    }

    fn apply_styles(&mut self) -> Result<(), Reason> {
        let mut ink: HashMap<String, PaintStyle> = HashMap::new();
        if let Some(style) = &self.default_style {
            for key in self.classes.keys() {
                lay(ink.entry(key.clone()).or_default(), style);
            }
        }
        let attached = std::mem::take(&mut self.attached);
        for (key, names) in attached {
            for (name, line) in names {
                let Some(style) = self.styles.get(&name) else {
                    return Err(Reason {
                        line,
                        what: "a style",
                    });
                };
                lay(ink.entry(key.clone()).or_default(), style);
            }
        }
        let pending_css = std::mem::take(&mut self.pending_css);
        for (name, context, style_name, line) in pending_css {
            let path = self.resolve(&name, &context, line, false)?;
            let key = self.full_key(&path).ok_or(Reason {
                line,
                what: "an unknown class",
            })?;
            let Some(style) = self.styles.get(&style_name) else {
                return Err(Reason {
                    line,
                    what: "a style",
                });
            };
            lay(ink.entry(key).or_default(), style);
        }
        let pending_direct = std::mem::take(&mut self.pending_direct);
        for (name, context, style, line) in pending_direct {
            let path = self.resolve(&name, &context, line, false)?;
            let key = self.full_key(&path).ok_or(Reason {
                line,
                what: "an unknown class",
            })?;
            lay(ink.entry(key).or_default(), &style);
        }
        for (key, style) in ink {
            let Some(path) = self.classes.get(&key).cloned() else {
                continue;
            };
            let node = self.node_mut(&path);
            if style.fill.is_some() {
                node.fill = style.fill;
            }
            if style.stroke.is_some() {
                node.line_color = style.stroke;
            }
            if style.thickness.is_some() {
                node.thickness = style.thickness;
            }
            if style.text.is_some() {
                node.text_color.clone_from(&style.text);
            }
            if style.dash.is_some() {
                node.border_dash = style.dash;
            }
        }
        Ok(())
    }

    fn resolve(
        &mut self,
        written: &str,
        context: &[String],
        line: usize,
        create: bool,
    ) -> Result<Vec<usize>, Reason> {
        let written = written.trim();
        if written.is_empty() {
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        }
        if let Some((ns, short)) = dotted(written) {
            let full = format!("{}.{}", ns.join("."), short);
            if let Some(path) = self.classes.get(&full).cloned() {
                return Ok(path);
            }
            if !create {
                return Err(Reason {
                    line,
                    what: "an unknown class",
                });
            }
            return self.ensure_class(&ns, &short, None, "", line);
        }
        if !context.is_empty() {
            let full = format!("{}.{}", context.join("."), written);
            if let Some(path) = self.classes.get(&full).cloned() {
                return Ok(path);
            }
        }
        if let Some(path) = self.classes.get(written).cloned() {
            return Ok(path);
        }
        let hits = self.shorts.get(written).cloned().unwrap_or_default();
        match (hits.len(), create) {
            (1, _) => Ok(self.classes[&hits[0]].clone()),
            (0, true) => self.ensure_class(context, written, None, "", line),
            (0, false) => Err(Reason {
                line,
                what: "an unknown class",
            }),
            _ => Err(Reason {
                line,
                what: "an ambiguous class name",
            }),
        }
    }

    fn ensure_class(
        &mut self,
        ns: &[String],
        short: &str,
        label: Option<String>,
        generic: &str,
        line: usize,
    ) -> Result<Vec<usize>, Reason> {
        let full = if ns.is_empty() {
            short.to_owned()
        } else {
            format!("{}.{}", ns.join("."), short)
        };
        let display = match &label {
            Some(label) => label.clone(),
            None if !generic.is_empty() => format!("{short}{generic}"),
            None => short.to_owned(),
        };
        if let Some(path) = self.classes.get(&full).cloned() {
            let node = self.node_mut(&path);
            let plain = node.label.value == short;
            if label.is_some() || (!generic.is_empty() && plain) {
                node.label = text_at(&display, line);
            }
            return Ok(path);
        }
        let home = if ns.is_empty() {
            Vec::new()
        } else {
            self.ensure_packages(ns, None, line)?;
            self.packages.get(&ns.join(".")).cloned().ok_or(Reason {
                line,
                what: "an unknown line",
            })?
        };
        let index = self.group(&home).nodes.len();
        self.group_mut(&home).nodes.push(Element {
            key: short.to_owned(),
            label: text_at(&display, line),
            stereotypes: Vec::new(),
            members: Vec::new(),
            shape: Shape::Class,
            fill: None,
            line_color: None,
            thickness: None,
            text_color: None,
            border_dash: None,
            explicit: true,
            composite: false,
            children: Group::default(),
            line,
        });
        let mut path = home;
        path.push(index);
        self.classes.insert(full.clone(), path.clone());
        self.shorts.entry(short.to_owned()).or_default().push(full);
        Ok(path)
    }

    fn ensure_packages(
        &mut self,
        segments: &[String],
        label: Option<String>,
        line: usize,
    ) -> Result<(), Reason> {
        let mut parent = Vec::new();
        for (i, seg) in segments.iter().enumerate() {
            let key = segments[..=i].join(".");
            if let Some(path) = self.packages.get(&key).cloned() {
                if i + 1 == segments.len()
                    && let Some(label) = &label
                {
                    let node = self.node_mut(&path);
                    if node.label.value == node.key {
                        node.label = text_at(label, line);
                    }
                }
                parent = path;
                continue;
            }
            let display = if i + 1 == segments.len() {
                label.clone().unwrap_or_else(|| seg.clone())
            } else {
                seg.clone()
            };
            let index = self.group(&parent).nodes.len();
            let element = Element {
                key: seg.clone(),
                label: text_at(&display, line),
                stereotypes: Vec::new(),
                members: Vec::new(),
                shape: Shape::Package,
                fill: None,
                line_color: None,
                thickness: None,
                text_color: None,
                border_dash: None,
                explicit: true,
                composite: true,
                children: Group {
                    direction: self.direction,
                    ..Group::default()
                },
                line,
            };
            self.group_mut(&parent).nodes.push(element);
            let mut path = parent.clone();
            path.push(index);
            self.packages.insert(key, path.clone());
            parent = path;
        }
        Ok(())
    }

    fn add_parts(&mut self, path: &[usize], body: &str, line: usize) -> Result<(), Reason> {
        let path = path.to_vec();
        for piece in body.split('\n') {
            for part in split_semi(piece) {
                if let Some(stereo) = stereo_inner(part) {
                    self.add_stereotypes(&path, &[stereo], line);
                } else {
                    self.add_member(&path, member_of(part, line)?);
                }
            }
        }
        Ok(())
    }

    fn add_member(&mut self, path: &[usize], member: Member) {
        self.node_mut(path).members.push(member);
    }

    fn add_stereotypes(&mut self, path: &[usize], stereotypes: &[String], line: usize) {
        let shape = richer(self.node_mut(path).shape, shape_of(stereotypes));
        let node = self.node_mut(path);
        node.shape = shape;
        for stereo in stereotypes {
            if !node
                .stereotypes
                .iter()
                .any(|have| have.value.eq_ignore_ascii_case(stereo))
            {
                node.stereotypes.push(text_at(stereo, line));
            }
        }
    }

    fn place(&mut self, rel: &Rel) -> Result<(), Reason> {
        let link = |from: usize, to: usize| Link {
            from,
            to,
            label: rel.label.as_deref().map(|text| text_at(text, rel.line)),
            from_card: rel.from_card.as_deref().map(|text| text_at(text, rel.line)),
            to_card: rel.to_card.as_deref().map(|text| text_at(text, rel.line)),
            from_head: rel.from_head,
            to_head: rel.to_head,
            color: None,
            dash: rel.dash,
            bold: false,
            line: rel.line,
        };
        let from = &rel.from;
        let to = &rel.to;
        if from.is_empty() || to.is_empty() {
            return Err(Reason {
                line: rel.line,
                what: "an unknown line",
            });
        }
        if from == to {
            let index = *from.last().expect("a class has a path");
            self.group_mut(&from[..from.len() - 1])
                .edges
                .push(link(index, index));
            return Ok(());
        }
        let home = from.len() - 1;
        if to.len() == from.len() && to[..home] == from[..home] {
            let from_i = *from.last().expect("a class has a path");
            let to_i = *to.last().expect("a class has a path");
            self.group_mut(&from[..home]).edges.push(link(from_i, to_i));
            return Ok(());
        }
        let Some((depth, a, b)) = meeting(from, to) else {
            return Err(Reason {
                line: rel.line,
                what: "a line between a box and something inside it",
            });
        };
        self.group_mut(&from[..depth]).crossings.push(Crossing {
            from: from[depth..].to_vec(),
            to: to[depth..].to_vec(),
            arrow: link(a, b),
        });
        Ok(())
    }

    fn full_key(&self, path: &[usize]) -> Option<String> {
        self.classes
            .iter()
            .find(|(_, found)| found.as_slice() == path)
            .map(|(key, _)| key.clone())
    }

    fn group(&self, path: &[usize]) -> &Group {
        let mut group = &self.root;
        for &index in path {
            group = &group.nodes[index].children;
        }
        group
    }

    fn group_mut(&mut self, path: &[usize]) -> &mut Group {
        let mut group = &mut self.root;
        for &index in path {
            group = &mut group.nodes[index].children;
        }
        group
    }

    fn node_mut(&mut self, path: &[usize]) -> &mut Element {
        let (last, home) = path.split_last().expect("a class has a path");
        &mut self.group_mut(home).nodes[*last]
    }
}

fn paint_direction(group: &mut Group, direction: Direction) {
    group.direction = direction;
    for node in &mut group.nodes {
        if node.composite {
            paint_direction(&mut node.children, direction);
        }
    }
}

fn note_box(key: String, text: String, line: usize) -> Element {
    Element {
        key,
        label: text_at(&text, line),
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
        line,
    }
}

fn lay(slot: &mut PaintStyle, style: &PaintStyle) {
    if style.fill.is_some() {
        slot.fill.clone_from(&style.fill);
    }
    if style.stroke.is_some() {
        slot.stroke.clone_from(&style.stroke);
    }
    if style.thickness.is_some() {
        slot.thickness = style.thickness;
    }
    if style.text.is_some() {
        slot.text.clone_from(&style.text);
    }
    if style.dash.is_some() {
        slot.dash = style.dash;
    }
}

struct Decl {
    short: String,
    generic: String,
    label: Option<String>,
    stereotypes: Vec<String>,
    css: Vec<String>,
    inline: Option<String>,
    open: bool,
}

fn parse_decl(input: &str, line: usize) -> Result<Decl, Reason> {
    let input = input.trim();
    let unknown = Reason {
        line,
        what: "an unknown line",
    };
    let (short, mut rest) = if let Some(rest) = input.strip_prefix('`') {
        let end = rest.find('`').ok_or(unknown)?;
        (
            rest[..end].to_owned(),
            rest[end + 1..].trim_start().to_owned(),
        )
    } else {
        let (ident, after) = take_ident(input).ok_or(Reason {
            line,
            what: "an unknown line",
        })?;
        (ident, after.trim_start().to_owned())
    };
    let mut generic = String::new();
    if rest.starts_with('~') {
        let len = generic_len(&rest).ok_or(Reason {
            line,
            what: "an unknown line",
        })?;
        generic = rest[..len].to_owned();
        rest = rest[len..].trim_start().to_owned();
    }
    let mut label = None;
    if rest.starts_with('[') {
        let (text, after) = bracket_label(&rest, line)?;
        label = Some(text);
        rest = after;
    }
    let mut stereotypes = Vec::new();
    while rest.starts_with("<<") {
        let Some(end) = rest.find(">>") else {
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        };
        let stereo = rest[2..end].trim();
        if stereo.is_empty() {
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        }
        stereotypes.push(stereo.to_owned());
        rest = rest[end + 2..].trim_start().to_owned();
    }
    let mut css = Vec::new();
    if let Some(after) = rest.strip_prefix(":::") {
        let end = after
            .find(|c: char| c.is_whitespace() || c == '{')
            .unwrap_or(after.len());
        css = after[..end]
            .split(',')
            .map(|name| name.trim().trim_end_matches(';').to_owned())
            .filter(|name| !name.is_empty())
            .collect();
        if css.is_empty() {
            return Err(Reason {
                line,
                what: "a style",
            });
        }
        rest = after[end..].trim_start().to_owned();
    }
    let (inline, open) = split_body(&rest, line)?;
    Ok(Decl {
        short,
        generic,
        label,
        stereotypes,
        css,
        inline,
        open,
    })
}

fn split_body(rest: &str, line: usize) -> Result<(Option<String>, bool), Reason> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Ok((None, false));
    }
    let Some(inner) = rest.strip_prefix('{') else {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    };
    let inner = inner.trim();
    if let Some(body) = inner.strip_suffix('}') {
        if body.contains(['{', '}']) {
            return Err(Reason {
                line,
                what: "an unknown line",
            });
        }
        let body = body.trim();
        return Ok(((!body.is_empty()).then(|| body.to_owned()), false));
    }
    if inner.contains(['{', '}']) {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    }
    Ok(((!inner.is_empty()).then(|| inner.to_owned()), true))
}

fn bracket_label(rest: &str, line: usize) -> Result<(String, String), Reason> {
    let Some(after) = rest.trim_start().strip_prefix('[') else {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    };
    let (text, after) = quoted(after.trim_start()).ok_or(Reason {
        line,
        what: "an unknown line",
    })?;
    let Some(after) = after.trim_start().strip_prefix(']') else {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    };
    Ok((text, after.trim_start().to_owned()))
}

fn member_of(text: &str, line: usize) -> Result<Member, Reason> {
    let (italic, underline, rest) = classifiers(text);
    if rest.is_empty() {
        return Err(Reason {
            line,
            what: "an unknown line",
        });
    }
    let slot = if rest.contains('(') {
        Slot::Method
    } else {
        Slot::Attribute
    };
    Ok(Member::Text {
        text: text_at(&rest, line),
        italic,
        underline,
        slot,
    })
}

/// `*` and `$` at the end of a member mark it abstract or static.
fn classifiers(text: &str) -> (bool, bool, String) {
    let mut italic = false;
    let mut underline = false;
    let mut rest = text.trim().to_owned();
    loop {
        let trimmed = rest.trim_end();
        if trimmed.len() > 1 && trimmed.ends_with('*') {
            italic = true;
            rest = trimmed[..trimmed.len() - 1].trim_end().to_owned();
            continue;
        }
        if trimmed.len() > 1 && trimmed.ends_with('$') {
            underline = true;
            rest = trimmed[..trimmed.len() - 1].trim_end().to_owned();
            continue;
        }
        break;
    }
    (italic, underline, rest)
}

fn shape_of(stereotypes: &[String]) -> Shape {
    let mut shape = Shape::Class;
    for stereo in stereotypes {
        let next = match stereo.to_ascii_lowercase().as_str() {
            "interface" => Shape::Interface,
            "enumeration" | "enum" => Shape::Enum,
            "abstract" => Shape::Abstract,
            _ => continue,
        };
        shape = richer(shape, next);
    }
    shape
}

fn richer(old: Shape, new: Shape) -> Shape {
    if rank(new) > rank(old) { new } else { old }
}

fn rank(shape: Shape) -> u8 {
    match shape {
        Shape::Interface => 3,
        Shape::Enum => 2,
        Shape::Abstract => 1,
        _ => 0,
    }
}

fn paint_style(text: &str, line: usize) -> Result<PaintStyle, Reason> {
    let mut style = PaintStyle::default();
    let mut any = false;
    // A dash array is `5 5` or `5,5`. The comma is the style separator
    // too, so a bare number stays with the dash that came before it.
    let mut parts: Vec<String> = Vec::new();
    for part in text.split(',') {
        let part = part.trim().trim_end_matches(';').trim();
        if part.is_empty() {
            continue;
        }
        let continues_dash = !part.contains(':')
            && parts.last().is_some_and(|prev| {
                prev.split_once(':')
                    .is_some_and(|(key, _)| key.trim().eq_ignore_ascii_case("stroke-dasharray"))
            });
        if continues_dash {
            let index = parts.len() - 1;
            parts[index].push(' ');
            parts[index].push_str(part);
        } else {
            parts.push(part.to_owned());
        }
    }
    for part in &parts {
        any = true;
        let Some((key, value)) = part.split_once(':') else {
            return Err(Reason {
                line,
                what: "a style",
            });
        };
        let value = value.trim().trim_end_matches(';').trim();
        if value.is_empty() {
            return Err(Reason {
                line,
                what: "a style",
            });
        }
        match key.trim().to_ascii_lowercase().as_str() {
            "fill" => {
                if crate::color::parse(value).is_err() {
                    return Err(Reason {
                        line,
                        what: "a color",
                    });
                }
                style.fill = Some(value.to_owned());
            }
            "stroke" => {
                if !matches!(crate::color::parse(value), Ok(Some(_))) {
                    return Err(Reason {
                        line,
                        what: "a color",
                    });
                }
                style.stroke = Some(value.to_owned());
            }
            "stroke-width" => {
                style.thickness = Some(px(value).ok_or(Reason {
                    line,
                    what: "a style",
                })?);
            }
            "color" => {
                if !matches!(crate::color::parse(value), Ok(Some(_))) {
                    return Err(Reason {
                        line,
                        what: "a color",
                    });
                }
                style.text = Some(value.to_owned());
            }
            "stroke-dasharray" => {
                style.dash = Some(dash_of(value).ok_or(Reason {
                    line,
                    what: "a style",
                })?);
            }
            _ => {
                return Err(Reason {
                    line,
                    what: "a style",
                });
            }
        }
    }
    if !any {
        return Err(Reason {
            line,
            what: "a style",
        });
    }
    Ok(style)
}

/// A length in pixels, from `4` or `4px`. Zero is a hairline. Wider than
/// the skin's thickest border is refused, so a style cannot paint over
/// the next box.
fn px(value: &str) -> Option<f32> {
    let value = value.trim();
    let value = value
        .strip_suffix("px")
        .or_else(|| value.strip_suffix("PX"))
        .unwrap_or(value)
        .trim();
    let n: f32 = value.parse().ok()?;
    (n.is_finite() && (0.0..=20.0).contains(&n)).then_some(n)
}

/// `5`, `5 5` or `5px 3px`. A comma in the source is a space by the time
/// it arrives here. A zero length is not a dash.
fn dash_of(value: &str) -> Option<(f32, f32)> {
    let mut parts = value.split_whitespace();
    let on = px(parts.next()?)?;
    let off = match parts.next() {
        Some(off) => px(off)?,
        None => on,
    };
    if parts.next().is_some() || on == 0.0 || off == 0.0 {
        return None;
    }
    Some((on, off))
}

fn dotted(written: &str) -> Option<(Vec<String>, String)> {
    if !written.contains('.') {
        return None;
    }
    let mut parts: Vec<&str> = written.split('.').collect();
    let short = parts.pop()?.to_owned();
    if short.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    Some((parts.iter().map(|part| (*part).to_owned()).collect(), short))
}

const REL: &[&str] = &[
    "<|--|>", "<|..|>", "<-->", "<..>", "*--*", "*..*", "o--o", "o..o", "()--", "--()", "<|--",
    "--|>", "<|..", "..|>", "*--", "--*", "*..", "..*", "o--", "--o", "o..", "..o", "-->", "<--",
    "..>", "<..", "--", "..",
];

fn find_rel(line: &str) -> Option<(usize, &'static str)> {
    let mask = mask_spans(line);
    let mut i = 0;
    while i < mask.len() {
        for token in REL {
            if mask[i..].starts_with(token) {
                return Some((i, token));
            }
        }
        let ch = mask[i..].chars().next().unwrap_or('\u{FFFD}');
        i += ch.len_utf8();
    }
    None
}

fn heads(token: &str) -> Option<(Head, Head, Dash)> {
    let dash = if token.contains("..") {
        Dash::Dashed
    } else {
        Dash::Solid
    };
    let glue = if token.contains("..") { ".." } else { "--" };
    let (left, right) = token.split_once(glue)?;
    Some((one_head(left)?, one_head(right)?, dash))
}

fn one_head(text: &str) -> Option<Head> {
    Some(match text {
        "" => Head::None,
        "<|" | "|>" => Head::Triangle,
        "*" => Head::DiamondFilled,
        "o" => Head::Diamond,
        "<" | ">" => Head::Arrow,
        _ => return None,
    })
}

fn split_left(text: &str) -> Option<(String, Option<String>)> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some((name, card)) = trailing_card(text) {
        return Some((bare_name(&token(name)?), Some(card)));
    }
    Some((bare_name(&token(text)?), None))
}

fn split_right(text: &str) -> Option<(String, Option<String>, Option<String>)> {
    let text = text.trim();
    let (card, rest) = if let Some((card, rest)) = leading_card(text) {
        (Some(card), rest)
    } else {
        (None, text)
    };
    let (name, label) = split_colon_label(rest);
    let name = bare_name(&token(name)?);
    let label = label
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned);
    Some((name, card, label))
}

fn trailing_card(text: &str) -> Option<(&str, String)> {
    let text = text.trim();
    let quote = *text.as_bytes().last()?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let body = &text[..text.len() - 1];
    let open = body.rfind(quote as char)?;
    if open == 0 || !body.as_bytes()[open - 1].is_ascii_whitespace() {
        return None;
    }
    let name = body[..open].trim();
    if name.is_empty() {
        return None;
    }
    Some((name, body[open + 1..].to_owned()))
}

fn leading_card(text: &str) -> Option<(String, &str)> {
    let text = text.trim();
    let quote = text.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &text[quote.len_utf8()..];
    let end = rest.find(quote)?;
    let after = rest[end + quote.len_utf8()..].trim_start();
    if after.is_empty() {
        return None;
    }
    Some((rest[..end].to_owned(), after))
}

fn token(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('`') {
        let end = rest.find('`')?;
        if rest[end + 1..].trim().is_empty() {
            return Some(rest[..end].to_owned());
        }
        return None;
    }
    if text.is_empty() || text.chars().any(char::is_whitespace) {
        return None;
    }
    Some(text.to_owned())
}

fn bare_name(token: &str) -> String {
    let Some(at) = token.find('~') else {
        return token.to_owned();
    };
    if generic_len(&token[at..]) == Some(token.len() - at) {
        token[..at].to_owned()
    } else {
        token.to_owned()
    }
}

fn take_ref(text: &str) -> Option<(String, &str)> {
    let text = text.trim();
    if let Some(rest) = text.strip_prefix('`') {
        let end = rest.find('`')?;
        return Some((bare_name(&rest[..end]), rest[end + 1..].trim_start()));
    }
    let end = text
        .find(|c: char| c.is_whitespace() || c == '"' || c == '\'')
        .unwrap_or(text.len());
    if end == 0 {
        return None;
    }
    Some((bare_name(&text[..end]), text[end..].trim_start()))
}

fn take_ident(text: &str) -> Option<(String, &str)> {
    let mut end = 0;
    for (index, ch) in text.char_indices() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    (end > 0).then(|| (text[..end].to_owned(), &text[end..]))
}

/// The byte length of a `~...~` generic, including nested `List~int~`.
fn generic_len(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if bytes.first() != Some(&b'~') {
        return None;
    }
    let mut depth = 1;
    let mut index = 1;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            let nested = bytes
                .get(index + 1)
                .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_');
            if nested {
                depth += 1;
            } else {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
        }
        index += 1;
    }
    None
}

fn split_colon_label(text: &str) -> (&str, Option<&str>) {
    let mut quote = None;
    let mut tick = false;
    for (index, ch) in text.char_indices() {
        if tick {
            if ch == '`' {
                tick = false;
            }
            continue;
        }
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            continue;
        }
        match ch {
            '`' => tick = true,
            '"' | '\'' => quote = Some(ch),
            ':' => return (text[..index].trim(), Some(text[index + 1..].trim())),
            _ => {}
        }
    }
    (text.trim(), None)
}

fn split_semi(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut quote = None;
    for (index, ch) in text.char_indices() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            quote = Some(ch);
            continue;
        }
        if ch == ';' {
            let part = text[start..index].trim();
            if !part.is_empty() {
                out.push(part);
            }
            start = index + 1;
        }
    }
    let part = text[start..].trim();
    if !part.is_empty() {
        out.push(part);
    }
    out
}

fn split_close(text: &str) -> (&str, bool) {
    let trimmed = text.trim();
    match trimmed.strip_suffix('}') {
        Some(rest) => (rest.trim(), true),
        None => (trimmed, false),
    }
}

fn stereo_inner(text: &str) -> Option<String> {
    let text = text.trim();
    let inner = text.strip_prefix("<<")?.strip_suffix(">>")?.trim();
    if inner.is_empty() || inner.contains(">>") {
        return None;
    }
    Some(inner.to_owned())
}

fn word<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    if text.len() < key.len() || !text[..key.len()].eq_ignore_ascii_case(key) {
        return None;
    }
    let rest = &text[key.len()..];
    if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(rest.trim_start())
}

fn ignored(text: &str) -> bool {
    let Some(first) = text.split_whitespace().next() else {
        return false;
    };
    matches!(
        first.to_ascii_lowercase().as_str(),
        "click" | "link" | "callback" | "acctitle" | "accdescr"
    )
}

fn mask_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote = None;
    let mut tick = false;
    for ch in line.chars() {
        let hide = if tick {
            if ch == '`' {
                tick = false;
            }
            true
        } else if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            true
        } else if ch == '`' {
            tick = true;
            true
        } else if ch == '"' || ch == '\'' {
            quote = Some(ch);
            true
        } else {
            false
        };
        if hide {
            for _ in 0..ch.len_utf8() {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn quoted(text: &str) -> Option<(String, &str)> {
    let text = text.trim();
    let quote = text.chars().next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &text[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some((
        breaks(&rest[..end]),
        rest[end + quote.len_utf8()..].trim_start(),
    ))
}

fn unquote(text: &str) -> String {
    let text = text.trim();
    if text.len() >= 2 {
        let bytes = text.as_bytes();
        let quote = bytes[0];
        if (quote == b'"' || quote == b'\'') && bytes[bytes.len() - 1] == quote {
            return text[1..text.len() - 1].to_owned();
        }
    }
    text.to_owned()
}

fn breaks(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::new();
    let mut at = 0;
    while at < text.len() {
        let rest = &lower[at..];
        if let Some(body) = text[at..].strip_prefix("\\n") {
            out.push('\n');
            at = text.len() - body.len();
            continue;
        }
        let size = if rest.starts_with("<br/>") {
            5
        } else if rest.starts_with("<br />") {
            6
        } else if rest.starts_with("<br>") {
            4
        } else {
            0
        };
        if size > 0 {
            out.push('\n');
            at += size;
            continue;
        }
        let ch = text[at..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        at += ch.len_utf8();
    }
    out
}

fn cut_comment(text: &str) -> String {
    let mut quote = None;
    let mut chars = text.chars().peekable();
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        if let Some(mark) = quote {
            if ch == mark {
                quote = None;
            }
            out.push(ch);
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            out.push(ch);
            continue;
        }
        if ch == '%' && chars.peek() == Some(&'%') {
            break;
        }
        out.push(ch);
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::super::{Dash, Direction, Head, Shape, Slot, draw};
    use super::{from_mermaid, mermaid_refusal};

    fn diagram(source: &str) -> super::super::Diagram {
        from_mermaid(source).unwrap_or_else(|| panic!("refused {source}"))
    }

    #[test]
    fn reads_the_library_example() {
        let diagram = diagram(include_str!("../../../../examples/library.mmd"));
        let library = &diagram.root.nodes[0];
        assert_eq!(library.key, "Library");
        assert_eq!(library.members.len(), 3);
        assert!(matches!(
            &library.members[2],
            super::super::Member::Text {
                slot: Slot::Method,
                ..
            }
        ));
        let holds = diagram
            .root
            .edges
            .iter()
            .find(|edge| {
                edge.label
                    .as_ref()
                    .is_some_and(|text| text.value == "holds")
            })
            .expect("holds");
        assert_eq!(
            holds.from_card.as_ref().map(|text| text.value.as_str()),
            Some("1")
        );
        assert_eq!(
            holds.to_card.as_ref().map(|text| text.value.as_str()),
            Some("*")
        );
        assert_eq!(holds.from_head, Head::Diamond);
        assert_eq!(holds.dash, Dash::Solid);
        let svg = draw(&diagram).unwrap();
        assert!(svg.contains("Library") && svg.contains("holds"), "{svg}");
    }

    #[test]
    fn reads_a_namespace_and_draws_it() {
        let diagram = diagram(include_str!(
            "../../../omaspec-core/tests/fixtures/class.mmd"
        ));
        assert_eq!(diagram.root.nodes[0].key, "Shell");
        assert!(diagram.root.nodes[0].composite);
        assert_eq!(diagram.root.nodes[0].children.nodes[0].key, "Bar");
        let svg = draw(&diagram).unwrap();
        assert!(svg.contains("Shell") && svg.contains("Bar") && svg.contains("Theme"));
    }

    #[test]
    fn generics_stay_on_the_label_and_off_the_name() {
        let diagram = diagram(
            "classDiagram\nclass Square~Shape~{\n  List~int~ position\n  getPoints() List~int~\n}\nSquare : -List~string~ messages\n",
        );
        let class = &diagram.root.nodes[0];
        assert_eq!(class.key, "Square");
        assert_eq!(class.label.value, "Square~Shape~");
        assert_eq!(class.members.len(), 3);
        assert!(class.members.iter().any(|member| matches!(
            member,
            super::super::Member::Text { text, .. } if text.value.contains("List~List") || text.value.contains("getPoints")
        )));
    }

    #[test]
    fn a_two_way_inheritance_has_both_triangles() {
        let diagram = diagram("classDiagram\nAnimal <|--|> Zebra\n");
        let edge = &diagram.root.edges[0];
        assert_eq!(edge.from_head, Head::Triangle);
        assert_eq!(edge.to_head, Head::Triangle);
        assert_eq!(edge.dash, Dash::Solid);
    }

    #[test]
    fn an_interface_annotation_sets_the_shape() {
        let diagram = diagram("classDiagram\nclass Shape{\n  <<interface>>\n  draw()\n}\n");
        let class = &diagram.root.nodes[0];
        assert_eq!(class.shape, Shape::Interface);
        assert_eq!(class.stereotypes[0].value, "interface");
        assert_eq!(class.members.len(), 1);
    }

    #[test]
    fn direction_and_a_title_and_a_note() {
        let source = "\
---
title: Animal example
---
classDiagram
direction LR
note \"From Duck till Zebra\"
class Duck
note for Duck \"can fly<br>can swim\"
";
        let diagram = diagram(source);
        assert_eq!(diagram.root.direction, Direction::Right);
        assert_eq!(
            diagram.frame.title.as_ref().map(|text| text.value.as_str()),
            Some("Animal example")
        );
        assert!(
            diagram
                .root
                .nodes
                .iter()
                .any(|node| node.shape == Shape::Note)
        );
        let duck = diagram
            .root
            .nodes
            .iter()
            .find(|node| node.key == "Duck")
            .expect("Duck");
        assert!(duck.label.value.contains("Duck") || duck.key == "Duck");
        assert_eq!(diagram.root.notes.len(), 1);
        assert!(diagram.root.notes[0].text.value.contains('\n'));
        let svg = draw(&diagram).unwrap();
        assert!(svg.contains("Animal example") && svg.contains("can fly"));
    }

    #[test]
    fn a_fill_is_drawn_and_a_width_is_refused() {
        let diagram = diagram("classDiagram\nclass Animal\nstyle Animal fill:#f9f\n");
        assert_eq!(diagram.root.nodes[0].fill.as_deref(), Some("#f9f"));
        let svg = draw(&diagram).unwrap();
        assert!(svg.contains("#FF99FF"), "{svg}");
        assert_eq!(
            mermaid_refusal("classDiagram\nclass Animal\nstyle Animal opacity:1\n"),
            Some((2, "a style"))
        );
    }

    #[test]
    fn a_stroke_width_stays_on_this_box() {
        let drawn = diagram(
            "classDiagram\n\
             class Animal\n\
             style Animal fill:#112233,stroke:#e68e0d,stroke-width:4px,color:#bebebe,stroke-dasharray: 5 5\n",
        );
        let animal = &drawn.root.nodes[0];
        assert_eq!(animal.fill.as_deref(), Some("#112233"));
        assert_eq!(animal.line_color.as_deref(), Some("#e68e0d"));
        assert_eq!(animal.thickness, Some(4.0));
        assert_eq!(animal.text_color.as_deref(), Some("#bebebe"));
        assert_eq!(animal.border_dash, Some((5.0, 5.0)));
        let svg = draw(&drawn).unwrap();
        assert!(
            svg.contains("stroke-width=\"4\"")
                && svg.contains("stroke-dasharray=\"5,5\"")
                && svg.contains("#112233")
                && svg.contains("#E68E0D")
                && svg.contains("#BEBEBE"),
            "{svg}"
        );
        let via = diagram(
            "classDiagram\n\
             classDef loud stroke-width:4px,fill:#112233\n\
             class Animal:::loud\n",
        );
        assert_eq!(via.root.nodes[0].thickness, Some(4.0));
        assert_eq!(
            mermaid_refusal("classDiagram\nclass Animal\nstyle Animal stroke-width:40px\n"),
            Some((2, "a style"))
        );
        assert!(
            from_mermaid("classDiagram\nclass Animal\nstyle Animal stroke-width:4px\n").is_some()
        );
        let commas = diagram("classDiagram\nclass Animal\nstyle Animal stroke-dasharray: 5,5\n");
        assert_eq!(commas.root.nodes[0].border_dash, Some((5.0, 5.0)));
    }

    #[test]
    fn lollipops_and_upward_directions_are_refused() {
        assert_eq!(
            mermaid_refusal("classDiagram\nbar ()-- foo\n"),
            Some((1, "a lollipop"))
        );
        assert_eq!(
            mermaid_refusal("classDiagram\ndirection RL\nclass A\n"),
            Some((1, "direction RL"))
        );
        assert!(from_mermaid("sequenceDiagram\nA->>B: hi\n").is_none());
        assert_eq!(mermaid_refusal("sequenceDiagram\nA->>B: hi\n"), None);
    }

    #[test]
    fn an_abstract_member_is_italic_and_a_static_one_is_underlined() {
        let diagram = diagram("classDiagram\nclass Box {\n  +mate()*\n  +count()$\n}\n");
        let members = &diagram.root.nodes[0].members;
        assert!(matches!(
            &members[0],
            super::super::Member::Text {
                italic: true,
                underline: false,
                text,
                ..
            } if text.value == "+mate()"
        ));
        assert!(matches!(
            &members[1],
            super::super::Member::Text {
                italic: false,
                underline: true,
                text,
                ..
            } if text.value == "+count()"
        ));
    }
}
