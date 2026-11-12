//! C4-PlantUML (`!include <C4/C4_Container>`), read into the structure
//! engine.
//!
//! The library itself is not read: Omaspec ships nothing of it and reads
//! nothing from the network. Its macros are known by name and each is
//! written as the PlantUML it stands for, on the line it was written on,
//! so the diagram is laid out by its ranks as PlantUML lays it out: a
//! person is a `person`, a system or a container a `rectangle` in the C4
//! colors, a database a `database`, a queue a `queue`, a boundary a
//! dashed `rectangle` around what it holds. `Rel_D` runs along the flow,
//! `Rel_R` within a rank (docs/adr/0027), `Rel_Back` from its second
//! element to its first, `Lay_R` is a hidden line.
//!
//! Every line that is no macro of the library is read as it is written.
//! Dynamic and sequence diagrams, sprites, tags and the styles the
//! library changes by tag are declined by name, and the Java PlantUML
//! draws them. Hints that only move boxes (`LAYOUT_TOP_DOWN()`,
//! `HIDE_STEREOTYPE()`, a direction across the flow between boxes of two
//! boundaries) change nothing that is said.

use std::collections::HashMap;

use crate::Declined;
use crate::c4::{Args, get, norm_name, parse_args, split_args};
use crate::color::{self, Color};
use crate::source::Line;

use super::Diagram;

/// A description is broken after about this many letters.
const WRAP: usize = 30;

/// Whether a body includes C4-PlantUML: the include lines are taken out
/// of `raw`, which is then read by [`parse`]. A dynamic or a sequence
/// diagram of the library is declined with its line.
pub(crate) fn take(raw: &mut Vec<Line<'_>>) -> Result<bool, Declined> {
    let mut found = false;
    let mut kept = Vec::with_capacity(raw.len());
    for line in raw.drain(..) {
        match library(line.text) {
            Some(name) => {
                found = true;
                if matches!(name.as_str(), "C4_Dynamic" | "C4_Sequence") {
                    let what = if name == "C4_Dynamic" {
                        "dynamic"
                    } else {
                        "sequence"
                    };
                    return Err(Declined::at(
                        line.no,
                        format!("C4-PlantUML's {what} diagrams are not supported yet"),
                    ));
                }
            }
            None => kept.push(line),
        }
    }
    *raw = kept;
    Ok(found)
}

/// The file of C4-PlantUML an include line names (`C4_Container`), from
/// the standard library (`<C4/C4_Container>`), the library's address, or
/// a copy of it (`C4_Container.puml`).
fn library(text: &str) -> Option<String> {
    let rest = text
        .strip_prefix("!includeurl")
        .or_else(|| text.strip_prefix("!include"))?;
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let target = rest.trim();
    let name = match target.strip_prefix("<C4/") {
        Some(inner) => inner.strip_suffix('>')?.trim_end_matches(".puml"),
        None => {
            let file = target.rsplit('/').next()?;
            file.strip_suffix(".puml")?
        }
    };
    let known = [
        "C4",
        "C4_Context",
        "C4_Container",
        "C4_Component",
        "C4_Deployment",
        "C4_Dynamic",
        "C4_Sequence",
    ];
    known.contains(&name).then(|| name.to_owned())
}

/// The diagram `lines` describe, its macros written out.
pub(crate) fn parse(lines: &[Line<'_>]) -> Result<Diagram, Declined> {
    let mut writer = Writer::default();
    for line in lines {
        writer.line(line)?;
    }
    writer.legend();
    let written: Vec<Line<'_>> = writer
        .out
        .iter()
        .map(|(model, text)| Line {
            text: text.as_str(),
            ..*model
        })
        .collect();
    super::parse(&written)
}

/// What an element is drawn as, by its macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Person,
    System,
    Container,
    Component,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Form {
    Box,
    Db,
    Queue,
}

/// A kind of element as the legend lists it.
type Sort = (Level, bool);

#[derive(Default)]
struct Writer<'a> {
    out: Vec<(Line<'a>, String)>,
    /// The flow runs to the right (`LAYOUT_LEFT_RIGHT()`).
    right: bool,
    /// The blocks open now: a boundary by its alias, any other block (a
    /// `package`, a `skinparam` block) as none.
    open: Vec<Option<String>>,
    /// Each element's boundaries, outermost first.
    home: HashMap<String, Vec<String>>,
    /// The kinds of element drawn, for the legend.
    sorts: Vec<Sort>,
    /// The line that asked for a legend.
    legend: Option<Line<'a>>,
    /// The defaults are written once, before the first line.
    started: bool,
}

impl<'a> Writer<'a> {
    fn push(&mut self, model: &Line<'a>, text: impl Into<String>) {
        self.out.push((*model, text.into()));
    }

    fn line(&mut self, line: &Line<'a>) -> Result<(), Declined> {
        if !self.started {
            self.started = true;
            // The C4 look; the writer's own skinparams come after it.
            self.push(line, "skinparam roundCorner 16");
        }
        let text = line.text;
        if text.eq_ignore_ascii_case("left to right direction") {
            self.right = true;
        }
        if text == "}" {
            self.open.pop();
            self.push(line, text);
            return Ok(());
        }
        let Some((name, inside, opens)) = call(text) else {
            // Another block opens: its `}` closes it, not a boundary.
            if text.ends_with('{') {
                self.open.push(None);
            }
            self.push(line, text);
            return Ok(());
        };
        let name = norm_name(name);
        let args = parse_args(&split_args(inside));
        let no = line.no;
        if let Some((level, form, external)) = element(&name) {
            let written = self.element(&args, level, form, external, no)?;
            self.push(line, written);
        } else if let Some(word) = boundary(&name) {
            let written = self.boundary(&args, &name, word, opens, no)?;
            self.push(line, written);
        } else if let Some(arrow) = relation(&name) {
            if let Some(written) = self.relation(&args, arrow, no)? {
                self.push(line, written);
            }
        } else if let Some(toward) = name.strip_prefix("lay_") {
            if let Some(written) = self.layout(&args, toward, no)? {
                self.push(line, written);
            }
        } else {
            match name.as_str() {
                "layout_top_down"
                | "hide_stereotype"
                | "show_person_outline"
                | "show_person_sprite"
                | "hide_person_sprite"
                | "show_person_portrait"
                | "show_element_descriptions" => {}
                "layout_left_right" | "layout_landscape" => {
                    self.right = true;
                    self.push(line, "left to right direction");
                }
                "layout_as_sketch" => self.push(line, "skinparam handwritten true"),
                "layout_with_legend" | "show_legend" | "show_floating_legend" => {
                    self.legend = Some(*line);
                }
                _ if styles(&name) => {
                    return Err(Declined::at(
                        no,
                        "C4-PlantUML's tags and styles are not supported yet",
                    ));
                }
                // Not a macro of the library: as it is written.
                _ => self.push(line, text),
            }
        }
        Ok(())
    }

    fn element(
        &mut self,
        args: &Args,
        level: Level,
        form: Form,
        external: bool,
        no: usize,
    ) -> Result<String, Declined> {
        let alias = alias(args, no)?;
        let label = args
            .positional
            .get(1)
            .cloned()
            .unwrap_or_else(|| alias.clone());
        // A person and a system say what they are only when told; a
        // container and a component always, with their technology.
        let technical = matches!(level, Level::Container | Level::Component);
        let at = |index: usize, key: &str| -> Option<String> {
            get(args, key)
                .map(str::to_owned)
                .or_else(|| args.positional.get(index).cloned())
                .filter(|text| !text.trim().is_empty())
        };
        let (techn, descr, sprite, tags, link) = if technical {
            (
                at(2, "techn"),
                at(3, "descr"),
                at(4, "sprite"),
                at(5, "tags"),
                at(6, "link"),
            )
        } else {
            (
                None,
                at(2, "descr"),
                at(3, "sprite"),
                at(4, "tags"),
                at(5, "link"),
            )
        };
        refuse(sprite, tags, no)?;
        let kind = get(args, "type")
            .map(str::to_owned)
            .filter(|text| !text.is_empty())
            .or_else(|| {
                technical.then(|| {
                    match level {
                        Level::Container => "Container",
                        _ => "Component",
                    }
                    .to_owned()
                })
            });
        let said = match (kind, techn) {
            (Some(kind), Some(techn)) => Some(format!("{kind}: {techn}")),
            (Some(kind), None) => Some(kind),
            (None, Some(techn)) => Some(techn),
            (None, None) => None,
        };
        let mut rows: Vec<String> = pieces(&label)
            .map(|piece| format!("<size:16><b>{piece}</b></size>"))
            .collect();
        if let Some(said) = said {
            rows.push(format!("<size:12>[{said}]</size>"));
        }
        if let Some(descr) = descr {
            rows.push(String::new());
            rows.extend(wrapped(&descr).map(|piece| format!("<size:13>{piece}</size>")));
        }
        let word = match (level, form) {
            (Level::Person, _) => "person",
            (_, Form::Db) => "database",
            (_, Form::Queue) => "queue",
            (_, Form::Box) => "rectangle",
        };
        let (fill, border) = colors(level, external);
        let ink = ink_on(fill);
        self.home.insert(alias.clone(), self.boundaries());
        if !self.sorts.contains(&(level, external)) {
            self.sorts.push((level, external));
        }
        Ok(format!(
            "{word} \"{}\" as {alias}{} #{fill};line:{border};text:{ink}",
            quoted(&rows.join("\\n"), no)?,
            linked(link.as_deref(), no)?
        ))
    }

    fn boundary(
        &mut self,
        args: &Args,
        name: &str,
        word: &str,
        opens: bool,
        no: usize,
    ) -> Result<String, Declined> {
        let alias = alias(args, no)?;
        let label = args
            .positional
            .get(1)
            .cloned()
            .unwrap_or_else(|| alias.clone());
        let deployment = name.starts_with("deployment_node") || name.starts_with("node");
        let at = |index: usize, key: &str| -> Option<String> {
            get(args, key)
                .map(str::to_owned)
                .or_else(|| args.positional.get(index).cloned())
                .filter(|text| !text.trim().is_empty())
        };
        // As C4-PlantUML writes them: `Deployment_Node(alias, label, type,
        // descr, sprite, tags, link)`, `Boundary(alias, label, type, tags,
        // link, descr)`, `System_Boundary(alias, label, tags, link, descr)`.
        let (kind, descr, sprite, tags, link) = if deployment {
            let (kind, descr) = (at(2, "type"), at(3, "descr"));
            (kind, descr, at(4, "sprite"), at(5, "tags"), at(6, "link"))
        } else if name == "boundary" {
            let kind = at(2, "type").or_else(|| Some(word.to_owned()));
            (kind, at(5, "descr"), None, at(3, "tags"), at(4, "link"))
        } else {
            let kind = get(args, "type")
                .map(str::to_owned)
                .or_else(|| Some(word.to_owned()));
            (kind, at(4, "descr"), None, at(2, "tags"), at(3, "link"))
        };
        refuse(sprite, tags, no)?;
        let mut rows: Vec<String> = pieces(&label)
            .map(|piece| format!("<b>{piece}</b>"))
            .collect();
        if let Some(kind) = kind {
            rows.push(format!("<size:12>[{kind}]</size>"));
        }
        if let Some(descr) = descr {
            rows.extend(wrapped(&descr).map(|piece| format!("<size:12>{piece}</size>")));
        }
        // A boundary is dashed, a deployment node drawn; neither is filled,
        // and both take the page's ink, so they read on any page.
        let border = if deployment {
            "#transparent"
        } else {
            "#transparent;line.dashed"
        };
        let body = if opens {
            self.open.push(Some(alias.clone()));
            " {"
        } else {
            ""
        };
        Ok(format!(
            "rectangle \"{}\" as {alias}{} {border}{body}",
            quoted(&rows.join("\\n"), no)?,
            linked(link.as_deref(), no)?
        ))
    }

    fn relation(&self, args: &Args, arrow: Arrow, no: usize) -> Result<Option<String>, Declined> {
        let (Some(from), Some(to)) = (args.positional.first(), args.positional.get(1)) else {
            return Err(Declined::at(no, "a relationship needs two elements"));
        };
        let at = |index: usize, key: &str| -> Option<String> {
            get(args, key)
                .map(str::to_owned)
                .or_else(|| args.positional.get(index).cloned())
                .filter(|text| !text.trim().is_empty())
        };
        let label = args.positional.get(2).cloned().unwrap_or_default();
        let (techn, descr, sprite, tags, link) = (
            at(3, "techn"),
            at(4, "descr"),
            at(5, "sprite"),
            at(6, "tags"),
            at(7, "link"),
        );
        refuse(sprite, tags, no)?;
        if link.is_some() {
            return Err(Declined::at(
                no,
                "a link on a relationship is not supported yet",
            ));
        }
        let mut rows: Vec<String> = pieces(&label)
            .map(|piece| format!("<b>{piece}</b>"))
            .collect();
        if let Some(techn) = techn {
            rows.push(format!("<size:12><i>[{techn}]</i></size>"));
        }
        if let Some(descr) = descr {
            rows.extend(wrapped(&descr).map(|piece| format!("<size:12>{piece}</size>")));
        }
        let toward = self.toward(arrow.toward, from, to);
        let middle = toward.map_or("-".to_owned(), |word| format!("-{word}-"));
        let line = match (arrow.back, arrow.both) {
            (false, false) => format!("{from} {middle}> {to}"),
            (false, true) => format!("{from} <{middle}> {to}"),
            // From the second to the first, the first ranked before it.
            (true, _) => format!("{from} <{middle} {to}"),
        };
        let line = if rows.is_empty() {
            line
        } else {
            format!("{line} : {}", rows.join("\\n"))
        };
        Ok(Some(line))
    }

    /// `Lay_D(a, b)`: a hidden line that places `b`.
    fn layout(&self, args: &Args, toward: &str, no: usize) -> Result<Option<String>, Declined> {
        let (Some(from), Some(to)) = (args.positional.first(), args.positional.get(1)) else {
            return Err(Declined::at(no, "a layout line needs two elements"));
        };
        let word = match toward {
            "d" | "down" | "distance" => "down",
            "u" | "up" => "up",
            "l" | "left" => "left",
            "r" | "right" => "right",
            _ => {
                return Err(Declined::at(
                    no,
                    format!("`Lay_{toward}` is not supported yet"),
                ));
            }
        };
        // Across the flow between two boundaries it would hold nothing.
        Ok(self
            .toward(Some(word), from, to)
            .map(|word| format!("{from} -[hidden]{word}- {to}")))
    }

    /// The boundaries open now, outermost first.
    fn boundaries(&self) -> Vec<String> {
        self.open.iter().flatten().cloned().collect()
    }

    /// The direction a line keeps: one across the flow between elements
    /// of different boundaries holds no rank, so it is dropped there.
    fn toward(&self, word: Option<&'static str>, from: &str, to: &str) -> Option<&'static str> {
        let word = word?;
        let across = matches!(word, "left" | "right") != self.right;
        let apart = match (self.home.get(from), self.home.get(to)) {
            (Some(a), Some(b)) => a != b,
            _ => false,
        };
        (!(across && apart)).then_some(word)
    }

    /// The legend `LAYOUT_WITH_LEGEND()` asks for: one small box of each
    /// kind of element drawn, in a dotted frame, one under the other.
    fn legend(&mut self) {
        let Some(model) = self.legend else { return };
        let mut sorts = self.sorts.clone();
        sorts.sort();
        if sorts.is_empty() {
            return;
        }
        self.push(
            &model,
            "rectangle \"<b>Legend</b>\" as c4_legend #transparent;line.dotted {",
        );
        for (index, &(level, external)) in sorts.iter().enumerate() {
            let (fill, border) = colors(level, external);
            let name = match level {
                Level::Person => "person",
                Level::System => "system",
                Level::Container => "container",
                Level::Component => "component",
            };
            let name = if external {
                format!("external {name}")
            } else {
                name.to_owned()
            };
            let ink = ink_on(fill);
            self.push(
                &model,
                format!(
                    "rectangle \"<size:12>{name}</size>\" as c4_legend_{index} #{fill};line:{border};text:{ink}"
                ),
            );
            if index > 0 {
                let line = format!("c4_legend_{} -[hidden]down- c4_legend_{index}", index - 1);
                self.push(&model, line);
            }
        }
        self.push(&model, "}");
    }
}

/// How a relationship macro is drawn.
#[derive(Clone, Copy)]
struct Arrow {
    toward: Option<&'static str>,
    both: bool,
    back: bool,
}

fn relation(name: &str) -> Option<Arrow> {
    let (both, rest) = match name.strip_prefix("birel") {
        Some(rest) => (true, rest),
        None => (false, name.strip_prefix("rel")?),
    };
    let (back, rest) = match rest.strip_prefix("_back") {
        Some(rest) => (true, rest),
        None => (false, rest),
    };
    let toward = match rest {
        "" => None,
        "_neighbor" => Some("right"),
        "_d" | "_down" => Some("down"),
        "_u" | "_up" => Some("up"),
        "_l" | "_left" => Some("left"),
        "_r" | "_right" => Some("right"),
        _ => return None,
    };
    Some(Arrow { toward, both, back })
}

/// The element a macro draws, and whether it is external.
fn element(name: &str) -> Option<(Level, Form, bool)> {
    let (name, external) = match name.strip_suffix("_ext") {
        Some(name) => (name, true),
        None => (name, false),
    };
    let (name, form) = if let Some(name) = name.strip_suffix("db") {
        (name, Form::Db)
    } else if let Some(name) = name.strip_suffix("queue") {
        (name, Form::Queue)
    } else {
        (name, Form::Box)
    };
    let level = match name {
        "person" if form == Form::Box => Level::Person,
        "system" => Level::System,
        "container" => Level::Container,
        "component" => Level::Component,
        _ => return None,
    };
    Some((level, form, external))
}

/// What a boundary macro says it is.
fn boundary(name: &str) -> Option<&'static str> {
    Some(match name {
        "boundary" => "Boundary",
        "enterprise_boundary" => "Enterprise",
        "system_boundary" => "System",
        "container_boundary" => "Container",
        "deployment_node" | "deployment_node_l" | "deployment_node_r" | "node" | "node_l"
        | "node_r" => "Node",
        _ => return None,
    })
}

/// The macros that change the look by tag or by element: not drawn yet.
fn styles(name: &str) -> bool {
    name.starts_with("add") && name.ends_with("tag")
        || name.starts_with("update") && name.ends_with("style")
        || matches!(
            name,
            "updatelegendtitle"
                | "setpropertyheader"
                | "withoutpropertyheader"
                | "addproperty"
                | "increment"
                | "setindex"
        )
}

/// The fill and the border of an element, in the colors of C4. The blue
/// of a container is a shade darker than C4-PlantUML's, so white reads
/// on it. The borders are middle tones that read on a light page and on
/// a dark one alike.
fn colors(level: Level, external: bool) -> (&'static str, &'static str) {
    match (level, external) {
        (Level::Person, false) => ("08427B", "4A8BCB"),
        (Level::System, false) => ("1168BD", "4A8BCB"),
        (Level::Container, false) => ("2C76BF", "4A8BCB"),
        (Level::Component, false) => ("85BBF0", "4A8BCB"),
        (Level::Person, true) => ("686868", "888888"),
        (Level::System, true) => ("999999", "888888"),
        (Level::Container, true) => ("B3B3B3", "888888"),
        (Level::Component, true) => ("CCCCCC", "888888"),
    }
}

/// White or black, whichever reads better on `fill`.
fn ink_on(fill: &str) -> &'static str {
    match color::parse(fill) {
        Ok(Some(fill)) if Color::WHITE.contrast(fill) >= Color::BLACK.contrast(fill) => "FFFFFF",
        _ => "000000",
    }
}

fn alias(args: &Args, no: usize) -> Result<String, Declined> {
    let alias = args
        .positional
        .first()
        .map(|text| text.trim())
        .unwrap_or_default();
    let fits = !alias.is_empty()
        && alias
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '.'));
    if fits {
        Ok(alias.to_owned())
    } else {
        Err(Declined::at(
            no,
            "an element needs a name of letters, digits and `_`",
        ))
    }
}

fn refuse(sprite: Option<String>, tags: Option<String>, no: usize) -> Result<(), Declined> {
    if sprite.is_some() {
        return Err(Declined::at(no, "sprites are not supported yet"));
    }
    if tags.is_some() {
        return Err(Declined::at(no, "C4-PlantUML's tags are not supported yet"));
    }
    Ok(())
}

/// ` [[url]]` for an element's link.
fn linked(link: Option<&str>, no: usize) -> Result<String, Declined> {
    match link {
        None => Ok(String::new()),
        Some(url) if url.contains([']', ' ', '"']) => Err(Declined::at(
            no,
            "a link with spaces or brackets is not supported yet",
        )),
        Some(url) => Ok(format!(" [[{url}]]")),
    }
}

/// Words written inside a quoted label: a quote of their own would end it.
fn quoted(text: &str, no: usize) -> Result<&str, Declined> {
    if text.contains('"') {
        Err(Declined::at(
            no,
            "a quote inside a C4 label is not supported yet",
        ))
    } else {
        Ok(text)
    }
}

/// The lines a text is written on: at `\n` as the writer broke it.
fn pieces(text: &str) -> impl Iterator<Item = &str> {
    text.split("\\n")
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
}

/// A description on lines of about [`WRAP`] letters, broken at spaces,
/// besides where the writer broke it.
fn wrapped(text: &str) -> impl Iterator<Item = String> + '_ {
    pieces(text).flat_map(|piece| {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in piece.split_whitespace() {
            if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > WRAP {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    })
}

/// `Name(args)` and whether a `{` follows; `None` for any other line.
fn call(text: &str) -> Option<(&str, &str, bool)> {
    let open = text.find('(')?;
    let name = text[..open].trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let mut quote = None;
    let mut depth = 0usize;
    for (at, ch) in text[open..].char_indices() {
        match (quote, ch) {
            (Some(mark), _) if ch == mark => quote = None,
            (Some(_), _) => {}
            (None, '"') => quote = Some('"'),
            (None, '(') => depth += 1,
            (None, ')') => {
                depth -= 1;
                if depth == 0 {
                    let close = open + at;
                    let after = text[close + 1..].trim();
                    return match after {
                        "" => Some((name, &text[open + 1..close], false)),
                        "{" => Some((name, &text[open + 1..close], true)),
                        _ => None,
                    };
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests;
