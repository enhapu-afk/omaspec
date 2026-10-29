//! `<style>` blocks: PlantUML's style sheets, in its CSS-like syntax.
//!
//! PlantUML turns every skinparam into a style and, of two settings that
//! reach an element, keeps the later one, whichever selector each names.
//! So the engine goes the other way: a declaration becomes the skinparams
//! of the elements it reaches, in its place among the `skinparam` lines,
//! and each diagram type's skin reads them as its own. An element takes
//! what it draws. A declaration that names an element and that no element
//! draws declines; one that reaches an element only from further up
//! (`root`, `document`, the diagram) leaves the rest of that element as it
//! is.

use std::collections::HashMap;

use crate::Declined;
use crate::common::{Declared, Skinparam};
use crate::source::{Line, starts_with_ci};

/// A property of PlantUML's styles, as far as the engine tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prop {
    BackGroundColor,
    LineColor,
    LineThickness,
    LineStyle,
    FontColor,
    FontSize,
    FontName,
    FontStyle,
    RoundCorner,
    Shadowing,
    HorizontalAlignment,
    /// One the engine does not draw yet: `Padding`, `MaximumWidth`, ….
    Other,
}

/// PlantUML's property names. Those of links (which decline elsewhere),
/// of exported names and of charts change nothing in what is drawn here.
const PROPS: [(&str, Option<Prop>); 30] = [
    ("Shadowing", Some(Prop::Shadowing)),
    ("FontName", Some(Prop::FontName)),
    ("FontColor", Some(Prop::FontColor)),
    ("FontSize", Some(Prop::FontSize)),
    ("FontStyle", Some(Prop::FontStyle)),
    ("FontWeight", Some(Prop::Other)),
    ("BackGroundColor", Some(Prop::BackGroundColor)),
    ("RoundCorner", Some(Prop::RoundCorner)),
    ("LineThickness", Some(Prop::LineThickness)),
    ("DiagonalCorner", Some(Prop::Other)),
    ("HyperLinkColor", None),
    ("HyperlinkUnderlineStyle", None),
    ("HyperlinkUnderlineThickness", None),
    ("HeadColor", Some(Prop::Other)),
    ("LineColor", Some(Prop::LineColor)),
    ("LineStyle", Some(Prop::LineStyle)),
    ("Padding", Some(Prop::Other)),
    ("Margin", Some(Prop::Other)),
    ("MaximumWidth", Some(Prop::Other)),
    ("MinimumWidth", Some(Prop::Other)),
    ("ExportedName", None),
    ("Image", Some(Prop::Other)),
    ("HorizontalAlignment", Some(Prop::HorizontalAlignment)),
    ("ShowStereotype", Some(Prop::Other)),
    ("ImagePosition", Some(Prop::Other)),
    ("MarkerShape", None),
    ("MarkerSize", None),
    ("MarkerColor", None),
    ("BarWidth", None),
    ("Width", None),
];

/// The names PlantUML's selectors know (its `SName`, without the `_` some
/// carry); any other word it reads as a stereotype.
const NAMES: [&str; 157] = [
    "action",
    "activationBox",
    "activity",
    "activityBar",
    "activityDiagram",
    "actor",
    "agent",
    "analog",
    "archimate",
    "arrow",
    "artifact",
    "binary",
    "body",
    "boundary",
    "box",
    "boxless",
    "business",
    "caption",
    "card",
    "cardinality",
    "circle",
    "classDiagram",
    "class",
    "clickable",
    "cloud",
    "closed",
    "collection",
    "collections",
    "component",
    "composite",
    "robust",
    "chenAttribute",
    "chenEerDiagram",
    "chenEntity",
    "chenRelationship",
    "concise",
    "clock",
    "componentDiagram",
    "constraintArrow",
    "control",
    "database",
    "day",
    "delay",
    "description",
    "destroy",
    "diamond",
    "document",
    "ebnf",
    "element",
    "entity",
    "end",
    "start",
    "stop",
    "file",
    "filesDiagram",
    "folder",
    "footer",
    "frame",
    "ganttDiagram",
    "generic",
    "goto",
    "group",
    "groupHeader",
    "header",
    "hexagon",
    "highlight",
    "hnote",
    "interface",
    "json",
    "jsonDiagram",
    "gitDiagram",
    "label",
    "leafNode",
    "legend",
    "lifeLine",
    "mainframe",
    "map",
    "milestone",
    "mindmapDiagram",
    "month",
    "name",
    "network",
    "newpage",
    "node",
    "note",
    "nwdiagDiagram",
    "packetdiagDiagram",
    "objectDiagram",
    "object",
    "package",
    "participant",
    "partition",
    "person",
    "port",
    "process",
    "qualified",
    "queue",
    "rectangle",
    "reference",
    "referenceHeader",
    "regex",
    "requirement",
    "rnote",
    "root",
    "rootNode",
    "saltDiagram",
    "separator",
    "sequenceDiagram",
    "server",
    "stack",
    "stateDiagram",
    "state",
    "stateBody",
    "stereotype",
    "storage",
    "swimlane",
    "task",
    "timegrid",
    "timeline",
    "timingDiagram",
    "title",
    "undone",
    "unstarted",
    "usecase",
    "verticalSeparator",
    "year",
    "visibilityIcon",
    "private",
    "protected",
    "public",
    "IEMandatory",
    "spot",
    "spotAnnotation",
    "spotInterface",
    "spotEnum",
    "spotProtocol",
    "spotStruct",
    "spotEntity",
    "spotException",
    "spotClass",
    "spotAbstractClass",
    "spotMetaClass",
    "spotStereotype",
    "spotDataClass",
    "spotRecord",
    "wbsDiagram",
    "yamlDiagram",
    "chartDiagram",
    "bar",
    "line",
    "area",
    "scatter",
    "axis",
    "hAxis",
    "vAxis",
    "grid",
    "annotation",
];

/// What a diagram type draws of one of its elements, as styles reach it.
#[derive(Debug, PartialEq, Eq)]
pub struct Element {
    /// The names PlantUML looks it up by, `root` aside; the last is its own.
    names: &'static [&'static str],
    /// The start of its skinparams: `Participant` in `ParticipantBorderColor`.
    prefix: &'static str,
    /// What it draws under the prefix and the usual skinparam names.
    props: &'static [Prop],
    /// What it draws under skinparams of their own.
    keys: &'static [(Prop, &'static str)],
}

impl Element {
    pub const fn new(
        names: &'static [&'static str],
        prefix: &'static str,
        props: &'static [Prop],
    ) -> Element {
        Element {
            names,
            prefix,
            props,
            keys: &[],
        }
    }

    pub const fn keys(
        names: &'static [&'static str],
        keys: &'static [(Prop, &'static str)],
    ) -> Element {
        Element {
            names,
            prefix: "",
            props: &[],
            keys,
        }
    }

    /// The skinparam that sets `prop`, if the element draws it.
    fn key(&self, prop: Prop) -> Option<String> {
        if let Some((_, key)) = self.keys.iter().find(|(p, _)| *p == prop) {
            return Some((*key).to_owned());
        }
        if !self.props.contains(&prop) {
            return None;
        }
        let usual = match prop {
            Prop::BackGroundColor => "BackgroundColor",
            Prop::LineColor => "BorderColor",
            Prop::LineThickness => "BorderThickness",
            Prop::LineStyle => "BorderStyle",
            Prop::FontColor => "FontColor",
            Prop::FontSize => "FontSize",
            Prop::FontName => "FontName",
            Prop::FontStyle => "FontStyle",
            Prop::RoundCorner => "RoundCorner",
            Prop::Shadowing => "Shadowing",
            // No usual name: each element that aligns says how.
            Prop::HorizontalAlignment | Prop::Other => return None,
        };
        Some(format!("{}{usual}", self.prefix))
    }

    /// Whether a declaration under `names` reaches the element: PlantUML
    /// applies a style to every element whose names hold all of the
    /// selector's.
    fn reached_by(&self, names: &[String]) -> bool {
        names
            .iter()
            .all(|n| n == "root" || self.names.iter().any(|m| m.eq_ignore_ascii_case(n)))
    }

    /// Whether `names` name the element itself.
    fn named_by(&self, names: &[String]) -> bool {
        self.names
            .last()
            .is_some_and(|own| names.iter().any(|n| own.eq_ignore_ascii_case(n)))
    }
}

/// What each diagram draws around it; every type lists its legend, which
/// PlantUML looks up with the diagram's name.
const DOCUMENT: [Element; 6] = [
    Element::keys(&["document"], &[(Prop::BackGroundColor, "BackgroundColor")]),
    Element::new(&["document", "title"], "Title", TEXT),
    Element::new(&["document", "header"], "Header", TEXT),
    Element::new(&["document", "footer"], "Footer", TEXT),
    Element::new(&["document", "caption"], "Caption", TEXT),
    Element::new(
        &["document", "mainframe"],
        "Mainframe",
        &[
            Prop::LineColor,
            Prop::LineThickness,
            Prop::FontColor,
            Prop::FontSize,
            Prop::FontStyle,
        ],
    ),
];

/// The properties of a text without a box.
pub const TEXT: &[Prop] = &[Prop::FontColor, Prop::FontSize, Prop::FontStyle];

/// The properties of a legend.
pub const LEGEND: &[Prop] = &[
    Prop::BackGroundColor,
    Prop::LineColor,
    Prop::LineThickness,
    Prop::FontColor,
    Prop::FontSize,
    Prop::FontStyle,
    Prop::RoundCorner,
];

/// One property a style sets, for one way through its selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The style names on the way, in lower case.
    names: Vec<String>,
    /// Why a selector on the way cannot be followed yet: a stereotype, a
    /// depth, a `*`.
    unsupported: Option<String>,
    prop: Prop,
    written: Declared,
    line: usize,
}

/// Whether `text` opens a style block: `<style>`, or `<style file=…>` and
/// its kin.
pub fn opens(text: &str) -> bool {
    starts_with_ci(text, "<style")
        && text["<style".len()..].starts_with(|c: char| c == '>' || c.is_whitespace())
}

/// The declarations of the style block that opens at `lines[at]`, and the
/// index of the line after it: `<style>` up to a line `</style>`, or both
/// on one line.
pub fn block(lines: &[Line], at: usize) -> Result<(Vec<Declaration>, usize), Declined> {
    let Line { no, text, .. } = lines[at];
    let Some(rest) = text["<style".len()..].strip_prefix('>') else {
        return Err(Declined::at(
            no,
            "styles from a file (`<style file=…>`) are not supported yet",
        ));
    };
    if let Some(end) = rest.to_ascii_lowercase().find("</style>") {
        if !rest[end + "</style>".len()..].trim().is_empty() {
            return Err(Declined::at(
                no,
                "nothing may follow `</style>` on its line",
            ));
        }
        return Ok((sheet(&[(no, &rest[..end])])?, at + 1));
    }
    if !rest.trim().is_empty() {
        return Err(Declined::at(
            no,
            "`<style>` goes on a line of its own, or with `</style>` at its end",
        ));
    }
    let end = lines[at + 1..]
        .iter()
        .position(|l| l.text.eq_ignore_ascii_case("</style>"))
        .ok_or_else(|| Declined::at(no, "`<style>` has no `</style>` line"))?;
    let body: Vec<(usize, &str)> = lines[at + 1..at + 1 + end]
        .iter()
        .map(|l| (l.no, l.text))
        .collect();
    Ok((sheet(&body)?, at + end + 2))
}

/// The skinparams `declarations` stand for in a diagram whose elements
/// are `elements`, in their order.
pub fn skinparams(
    declarations: &[Declaration],
    elements: &[Element],
) -> Result<Vec<Skinparam>, Declined> {
    let mut out = Vec::new();
    for d in declarations {
        let reached: Vec<&Element> = DOCUMENT
            .iter()
            .chain(elements)
            .filter(|e| e.reached_by(&d.names))
            .collect();
        if reached.is_empty() {
            continue;
        }
        let fail = |why: String| Declined::at(d.line, why);
        if let Some(why) = &d.unsupported {
            return Err(fail(why.clone()));
        }
        let value = d.written.value.as_str();
        let param = |key: &str, value: &str| Skinparam {
            key: key.to_owned(),
            value: value.to_owned(),
            line: d.line,
            declared: Some(d.written.clone()),
        };
        // `root` reaches everything; for these PlantUML's own skinparams
        // say the same.
        let root = d.names.iter().all(|n| n == "root");
        match d.prop {
            Prop::Shadowing if no_shadow(value) => continue,
            Prop::Shadowing => {
                return Err(fail(format!("`{}`: shadows are not drawn yet", d.written)));
            }
            Prop::FontName if root => {
                out.push(param("defaultFontName", value));
                continue;
            }
            Prop::FontName => {
                return Err(fail(format!(
                    "`{}`: a font name is supported for `root` only so far",
                    d.written
                )));
            }
            Prop::RoundCorner if root => {
                out.push(param("roundCorner", value));
                continue;
            }
            _ => {}
        }
        let value = skinparam_value(d.prop, value);
        let mut taken = false;
        for e in &reached {
            if let Some(key) = e.key(d.prop) {
                out.push(param(&key, &value));
                taken = true;
            }
        }
        if !taken && reached.iter().any(|e| e.named_by(&d.names)) {
            return Err(fail(format!("`{}` is not supported yet", d.written)));
        }
    }
    Ok(out)
}

/// Whether a `Shadowing` value means none.
fn no_shadow(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "false" | "no" | "none")
        || value.parse::<f32>().is_ok_and(|v| v == 0.0)
}

/// A style's value as the skinparam takes it.
fn skinparam_value(prop: Prop, value: &str) -> String {
    match prop {
        Prop::BackGroundColor | Prop::LineColor | Prop::FontColor
            if value.eq_ignore_ascii_case("none") =>
        {
            "transparent".to_owned()
        }
        // PlantUML writes the skinparams' `dotted` and `dashed` like this.
        Prop::LineStyle if matches!(value, "1;3" | "1-3") => "dotted".to_owned(),
        Prop::LineStyle if matches!(value, "7;7" | "7-7") => "dashed".to_owned(),
        _ => value.to_owned(),
    }
}

/// Nested blocks deeper than this decline.
const MAX_DEPTH: usize = 50;

/// A style sheet that would set more than this many properties declines;
/// selector lists inside selector lists multiply.
const MAX_DECLARATIONS: usize = 10_000;

/// A block as written: its selectors, its own properties, the blocks in it.
#[derive(Default)]
struct Rule {
    selectors: Vec<Selector>,
    /// The line of its selectors.
    line: usize,
    props: Vec<Property>,
    children: Vec<Rule>,
}

struct Selector {
    /// As written.
    raw: String,
    /// The style name in lower case, or why it cannot be followed yet.
    name: Result<String, String>,
}

/// One style value. `seen` is false when the property had nothing after
/// it; `""` is seen and empty.
struct ReadValue {
    text: String,
    seen: bool,
}

struct Property {
    name: String,
    prop: Prop,
    value: String,
    line: usize,
}

/// The declarations of a style sheet whose lines are `body`.
fn sheet(body: &[(usize, &str)]) -> Result<Vec<Declaration>, Declined> {
    let mut chars = Vec::new();
    for &(no, text) in body {
        chars.extend(text.chars().map(|c| (c, no)));
        chars.push(('\n', no));
    }
    let mut parser = Parser {
        chars,
        at: 0,
        variables: HashMap::new(),
        open: Vec::new(),
        rules: Vec::new(),
    };
    parser.run()?;
    let mut out = Vec::new();
    for rule in &parser.rules {
        flatten(rule, &[Vec::new()], &mut out)?;
    }
    Ok(out)
}

/// The declarations of `rule` for every way through the selectors above it
/// (`paths`), then those of the blocks inside it, in the order PlantUML
/// gives them their rank.
fn flatten(
    rule: &Rule,
    paths: &[Vec<&Selector>],
    out: &mut Vec<Declaration>,
) -> Result<(), Declined> {
    let mut longer = Vec::new();
    for path in paths {
        for selector in &rule.selectors {
            if longer.len() >= MAX_DECLARATIONS {
                return Err(Declined::at(
                    rule.line,
                    "this `<style>` nests too many selector lists",
                ));
            }
            let mut path = path.clone();
            path.push(selector);
            longer.push(path);
        }
    }
    for p in &rule.props {
        for path in &longer {
            if out.len() >= MAX_DECLARATIONS {
                return Err(Declined::at(p.line, "this `<style>` sets too much"));
            }
            let unsupported = path.iter().find_map(|s| s.name.as_ref().err()).cloned();
            let selector = path.last().map_or("", |s| s.raw.as_str());
            out.push(Declaration {
                names: path
                    .iter()
                    .filter_map(|s| s.name.as_ref().ok())
                    .cloned()
                    .collect(),
                unsupported,
                prop: p.prop,
                written: Declared {
                    selector: selector.to_owned(),
                    property: p.name.clone(),
                    value: p.value.clone(),
                },
                line: p.line,
            });
        }
    }
    for child in &rule.children {
        flatten(child, &longer, out)?;
    }
    Ok(())
}

/// Reads a style sheet the way PlantUML's parser does: lenient about a
/// missing or an extra `}`, strict about a value it refuses (`{`, a `*`
/// of its own, a property with nothing after it).
struct Parser {
    /// The sheet's characters with their lines, a newline after each line.
    chars: Vec<(char, usize)>,
    at: usize,
    variables: HashMap<String, String>,
    open: Vec<Rule>,
    rules: Vec<Rule>,
}

impl Parser {
    fn peek(&self, ahead: usize) -> char {
        self.chars.get(self.at + ahead).map_or('\0', |&(c, _)| c)
    }

    fn line(&self) -> usize {
        self.chars
            .get(self.at)
            .or(self.chars.last())
            .map_or(0, |&(_, no)| no)
    }

    fn bump(&mut self) {
        self.at += 1;
    }

    fn run(&mut self) -> Result<(), Declined> {
        loop {
            self.skip_blank();
            match self.peek(0) {
                '\0' => break,
                '}' => {
                    self.bump();
                    self.close();
                }
                '@' => {
                    return Err(Declined::at(
                        self.line(),
                        "`@media` and other `@` rules in `<style>` are not supported yet",
                    ));
                }
                ':' => {
                    // A selector may start with a colon: `:depth(1) {`.
                    self.bump();
                    let head = self.identifier();
                    self.selector(head)?;
                }
                _ => self.statement()?,
            }
        }
        while !self.open.is_empty() {
            self.close();
        }
        Ok(())
    }

    /// Closes the innermost block; an extra `}` changes nothing.
    fn close(&mut self) {
        let Some(rule) = self.open.pop() else {
            return;
        };
        match self.open.last_mut() {
            Some(parent) => parent.children.push(rule),
            None => self.rules.push(rule),
        }
    }

    fn statement(&mut self) -> Result<(), Declined> {
        let line = self.line();
        let c = self.peek(0);
        let head = self.identifier();
        if head.is_empty() {
            return Err(Declined::at(
                line,
                format!("`{c}` is not understood here in `<style>`"),
            ));
        }
        if let Some(name) = head.strip_prefix("--") {
            let value = self.value()?;
            let value = self.resolve(&value.text);
            self.variables.insert(name.to_owned(), value);
            return Ok(());
        }
        self.skip_blank();
        if matches!(self.peek(0), ',' | '*' | '{') {
            return self.selector(head);
        }
        self.property(head, line)
    }

    fn selector(&mut self, first: String) -> Result<(), Declined> {
        let line = self.line();
        let mut raws = vec![first];
        self.skip_blank();
        while self.peek(0) == ',' {
            self.bump();
            self.skip_blank();
            raws.push(self.identifier());
            self.skip_blank();
        }
        let star = self.peek(0) == '*';
        if star {
            self.bump();
            self.skip_blank();
        }
        if self.peek(0) != '{' {
            return Err(Declined::at(
                line,
                format!("`{}` needs a `{{` after it in `<style>`", raws.join(", ")),
            ));
        }
        self.bump();
        if self.open.len() >= MAX_DEPTH {
            return Err(Declined::at(line, "this `<style>` nests too deep"));
        }
        let mut selectors: Vec<Selector> = raws.into_iter().map(classify).collect();
        if star {
            for s in &mut selectors {
                s.name = Err("`*` in `<style>` is not supported yet".to_owned());
            }
        }
        self.open.push(Rule {
            selectors,
            line,
            ..Rule::default()
        });
        Ok(())
    }

    fn property(&mut self, name: String, line: usize) -> Result<(), Declined> {
        let read = self.value()?;
        // PlantUML's "parsing": the next token is not a value (`LineColor }`
        // or `LineColor` before `}`). An empty quoted value is still one.
        if !read.seen {
            return Err(Declined::at(
                line,
                format!("`{name}` in `<style>` needs a value"),
            ));
        }
        if self.open.is_empty() {
            return Err(Declined::at(
                line,
                format!("`{name}` in `<style>` needs a selector around it, as in `root {{ … }}`"),
            ));
        }
        // An unknown property changes nothing in PlantUML either.
        let Some(prop) = PROPS
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(&name))
            .and_then(|&(_, prop)| prop)
        else {
            return Ok(());
        };
        let value = self.resolve(&read.text);
        if value.is_empty() {
            return Ok(());
        }
        if let Some(rule) = self.open.last_mut() {
            rule.props.push(Property {
                name,
                prop,
                value,
                line,
            });
        }
        Ok(())
    }

    /// A name, or a quoted string. A stereotype's name (`.static lib`) may
    /// hold spaces.
    fn identifier(&mut self) -> String {
        if self.peek(0) == '"' {
            return self.quoted();
        }
        let stereotype = self.peek(0) == '.';
        let mut out = String::new();
        loop {
            let c = self.peek(0);
            if c == ' ' && stereotype {
                out.push(c);
                self.bump();
                continue;
            }
            if matches!(
                c,
                '\0' | ' ' | '\t' | '\n' | '\r' | '{' | '}' | ';' | ',' | ':' | '*'
            ) {
                break;
            }
            out.push(c);
            self.bump();
        }
        if stereotype {
            out.trim().to_owned()
        } else {
            out
        }
    }

    fn quoted(&mut self) -> String {
        self.bump();
        let mut out = String::new();
        while !matches!(self.peek(0), '\0' | '"') {
            out.push(self.peek(0));
            self.bump();
        }
        if self.peek(0) == '"' {
            self.bump();
        }
        out
    }

    /// A value, the way PlantUML's style parser reads one. Words are joined
    /// with a space, a `:` must be followed by a word, and quotes come off.
    /// `{` or a `*` of its own is its "bad definition"; `}` and the end of
    /// the line stop the value and stay for whatever is next. `seen` is
    /// false when nothing was there (`LineColor }`), and true for `""`.
    fn value(&mut self) -> Result<ReadValue, Declined> {
        self.skip_spaces();
        while self.peek(0) == ':' {
            self.bump();
            self.skip_spaces();
        }
        let mut out = String::new();
        let mut seen = false;
        loop {
            self.skip_spaces();
            match self.peek(0) {
                '\0' | '\n' | '\r' | '}' => break,
                ';' => {
                    self.bump();
                    break;
                }
                '"' => {
                    seen = true;
                    out.push_str(&self.quoted());
                }
                ',' => {
                    seen = true;
                    out.push(',');
                    self.bump();
                }
                ':' => {
                    self.bump();
                    self.skip_spaces();
                    match self.peek(0) {
                        '"' => {
                            seen = true;
                            out.push(':');
                            out.push_str(&self.quoted());
                        }
                        c if Self::word_char(c) => {
                            seen = true;
                            out.push(':');
                            out.push_str(&self.style_word());
                        }
                        _ => {
                            return Err(Declined::at(
                                self.line(),
                                "`<style>` has a bad definition",
                            ));
                        }
                    }
                }
                '{' | '*' => {
                    return Err(Declined::at(self.line(), "`<style>` has a bad definition"));
                }
                c if Self::word_char(c) => {
                    seen = true;
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(&self.style_word());
                }
                c => {
                    return Err(Declined::at(
                        self.line(),
                        format!("`{c}` is not understood here in `<style>`"),
                    ));
                }
            }
        }
        Ok(ReadValue {
            text: out.trim().to_owned(),
            seen,
        })
    }

    /// A word of a value. `*` stays on it (`red*`); a `*` after a space is
    /// its own token and [`Self::value`] rejects it.
    fn style_word(&mut self) -> String {
        let mut out = String::new();
        while Self::word_char(self.peek(0)) {
            out.push(self.peek(0));
            self.bump();
        }
        out
    }

    /// What may sit inside one style word. The breaks are PlantUML's.
    fn word_char(c: char) -> bool {
        !matches!(
            c,
            '\0' | ' ' | '\t' | '\n' | '\r' | '{' | '}' | ';' | ',' | ':' | '"'
        )
    }

    /// `value` with every `var(--name)` replaced by what the sheet set
    /// before; an unknown one stays.
    fn resolve(&self, value: &str) -> String {
        let mut out = String::new();
        let mut rest = value;
        while let Some(at) = rest.find("var(") {
            out.push_str(&rest[..at]);
            let after = &rest[at + "var(".len()..];
            let Some(close) = after.find(')') else {
                out.push_str(&rest[at..]);
                return out;
            };
            let name = after[..close].trim().trim_start_matches('-');
            match self.variables.get(name) {
                Some(v) => out.push_str(v),
                None => out.push_str(&rest[at..at + "var(".len() + close + 1]),
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        out
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek(0), ' ' | '\t') {
            self.bump();
        }
    }

    /// Spaces, line ends and `//`, `/* */` and `/' '/` comments.
    fn skip_blank(&mut self) {
        loop {
            match (self.peek(0), self.peek(1)) {
                (' ' | '\t' | '\n' | '\r', _) => self.bump(),
                ('/', '/') => {
                    while !matches!(self.peek(0), '\0' | '\n') {
                        self.bump();
                    }
                }
                ('/', '*') => self.skip_comment('*'),
                ('/', '\'') => self.skip_comment('\''),
                _ => return,
            }
        }
    }

    /// Skips a comment from its `/` up to `mark` and `/`.
    fn skip_comment(&mut self, mark: char) {
        self.bump();
        self.bump();
        while self.peek(0) != '\0' {
            if self.peek(0) == mark && self.peek(1) == '/' {
                self.bump();
                self.bump();
                return;
            }
            self.bump();
        }
    }
}

/// What PlantUML makes of a selector word.
fn classify(raw: String) -> Selector {
    let name = if raw.starts_with('.') {
        Err(format!(
            "styles for stereotypes (`{raw}`) are not supported yet"
        ))
    } else if raw.starts_with("depth(") && raw.ends_with(')') {
        Err(format!("`{raw}` in `<style>` is not supported yet"))
    } else if let Some(name) = NAMES.iter().find(|n| n.eq_ignore_ascii_case(&raw)) {
        Ok(name.to_ascii_lowercase())
    } else {
        Err(format!(
            "`{raw}` is no style name PlantUML knows, so it reads it as a stereotype; \
             styles for stereotypes are not supported yet"
        ))
    };
    Selector { raw, name }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::body;

    /// The declarations of a `<style>` block, as `selector Property value`
    /// with the names on the way.
    fn read(sheet: &str) -> Result<Vec<(String, String, usize)>, Declined> {
        let source = format!("@startuml\n<style>\n{sheet}\n</style>\nA -> B\n@enduml");
        let (_, lines) = body(&source).unwrap();
        let (decls, next) = block(&lines, 0)?;
        assert_eq!(lines[next].text, "A -> B");
        Ok(decls
            .into_iter()
            .map(|d| {
                (
                    d.names.join("."),
                    format!("{} {}", d.written.property, d.written.value),
                    d.line,
                )
            })
            .collect())
    }

    fn pairs(sheet: &str) -> Vec<(String, String)> {
        read(sheet)
            .unwrap()
            .into_iter()
            .map(|(names, decl, _)| (names, decl))
            .collect()
    }

    fn own(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect()
    }

    #[test]
    fn reads_nested_blocks_lists_and_both_separators() {
        assert_eq!(
            pairs(
                "root {\n  FontColor red\n  FontSize: 12;\n}\nsequenceDiagram {\n  participant, actor {\n    BackGroundColor #EEE ; LineColor blue\n  }\n}"
            ),
            own(&[
                ("root", "FontColor red"),
                ("root", "FontSize 12"),
                ("sequencediagram.participant", "BackGroundColor #EEE"),
                ("sequencediagram.actor", "BackGroundColor #EEE"),
                ("sequencediagram.participant", "LineColor blue"),
                ("sequencediagram.actor", "LineColor blue"),
            ])
        );
        // A block on one line, a property after its block.
        assert_eq!(
            pairs("arrow { LineColor red }\nnote{FontSize 9}"),
            own(&[("arrow", "LineColor red"), ("note", "FontSize 9")])
        );
    }

    #[test]
    fn a_block_s_own_properties_come_before_those_of_its_blocks() {
        // PlantUML ranks a block's properties before those inside it, so
        // `participant` keeps red although `blue` is written later.
        assert_eq!(
            pairs("root {\n  participant { FontColor red }\n  FontColor blue\n}"),
            own(&[
                ("root", "FontColor blue"),
                ("root.participant", "FontColor red"),
            ])
        );
    }

    #[test]
    fn reads_variables_comments_quotes_and_lines() {
        let decls = read(
            "--accent: #E68E0D;\n// a comment\nroot {\n  /* one\n  two */ FontName \"DejaVu Sans\"\n  LineColor var(--accent)\n  FontColor var(--unknown)\n}",
        )
        .unwrap();
        assert_eq!(
            decls,
            [
                ("root".to_owned(), "FontName DejaVu Sans".to_owned(), 6),
                ("root".to_owned(), "LineColor #E68E0D".to_owned(), 7),
                ("root".to_owned(), "FontColor var(--unknown)".to_owned(), 8),
            ]
        );
    }

    #[test]
    fn is_lenient_where_plantuml_is() {
        // A missing `}` closes at the end, an extra one changes nothing,
        // an unknown property is ignored.
        assert_eq!(
            pairs("}\nroot {\n  Wobble 3\n  arrow {\n    LineColor red"),
            own(&[("root.arrow", "LineColor red")])
        );
        // `++` is part of the name, `*` glued to a word stays, and `{`
        // inside quotes is text. PlantUML draws all three.
        assert_eq!(
            pairs(
                "document {\n  Back++GroundColor #111111\n}\narrow { LineColor red* }\nroot { FontName \"a { b\" }"
            ),
            own(&[("arrow", "LineColor red*"), ("root", "FontName a { b")])
        );
    }

    #[test]
    fn keeps_what_it_cannot_follow_for_later() {
        let source = "@startuml\n<style>\n.warn { BackGroundColor red }\nfoo { FontColor red }\nmindmapDiagram { :depth(1) { FontSize 9 } }\nnode * { FontColor red }\n</style>\n@enduml";
        let (_, lines) = body(source).unwrap();
        let (decls, _) = block(&lines, 0).unwrap();
        let why: Vec<&str> = decls
            .iter()
            .map(|d| d.unsupported.as_deref().unwrap())
            .collect();
        assert!(why[0].contains("stereotypes (`.warn`)"), "{}", why[0]);
        assert!(why[1].contains("`foo` is no style name"), "{}", why[1]);
        assert!(why[2].contains("`depth(1)`"), "{}", why[2]);
        assert!(why[3].contains("`*`"), "{}", why[3]);
    }

    #[test]
    fn single_lines_and_errors() {
        let source = "@startuml\n<style>root { FontColor red }</style>\nA -> B\n@enduml";
        let (_, lines) = body(source).unwrap();
        let (decls, next) = block(&lines, 0).unwrap();
        assert_eq!((decls.len(), next), (1, 1));

        for (sheet, line, part) in [
            ("FontColor red", 2, "needs a selector"),
            ("root {\n  FontColor red;;\n}", 3, "not understood"),
            ("root {\n  LineColor }\n}", 3, "needs a value"),
            ("root {\n  LineColor\n}\n", 3, "needs a value"),
            (
                "arrow {\n  LineColor title { FontSize 18 }\n}",
                3,
                "bad definition",
            ),
            ("arrow { LineColor red * }", 2, "bad definition"),
            ("root, arrow\nFontColor red", 2, "needs a `{`"),
            (
                "@media (prefers-color-scheme:dark) {\n  root { FontColor white }\n}",
                2,
                "`@media`",
            ),
        ] {
            let err = read(sheet).unwrap_err();
            assert_eq!(err.line, Some(line), "{sheet}");
            assert!(err.reason.contains(part), "{sheet}: {}", err.reason);
        }
        for (source, part) in [
            ("<style>\nroot { FontColor red }", "no `</style>`"),
            ("<style file=dark.css>", "from a file"),
            ("<style> root {", "line of its own"),
            ("<style>root{}</style> x", "nothing may follow"),
        ] {
            let source = format!("@startuml\n{source}\n@enduml");
            let (_, lines) = body(&source).unwrap();
            let err = block(&lines, 0).unwrap_err();
            assert_eq!(err.line, Some(1), "{source}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
        assert!(opens("<style>") && opens("<STYLE file=x>"));
        assert!(!opens("<styles>") && !opens("<stylo>"));
    }

    #[test]
    fn selector_lists_cannot_multiply_without_end() {
        let sheet = "a, b, c, d, e, f, g, h, i, j {\n".repeat(8) + "FontColor red";
        let err = read(&sheet).unwrap_err();
        assert!(err.reason.contains("too"), "{}", err.reason);
    }

    const ELEMENTS: &[Element] = &[
        Element::new(
            &["element", "sequenceDiagram", "participant"],
            "Participant",
            &[Prop::BackGroundColor, Prop::FontColor],
        ),
        Element::keys(
            &["element", "sequenceDiagram", "arrow"],
            &[(Prop::LineColor, "ArrowColor")],
        ),
        Element::new(&["document", "sequenceDiagram", "legend"], "Legend", LEGEND),
    ];

    fn params(sheet: &str) -> Result<Vec<(String, String)>, Declined> {
        let source = format!("@startuml\n<style>\n{sheet}\n</style>\n@enduml");
        let (_, lines) = body(&source).unwrap();
        let (decls, _) = block(&lines, 0)?;
        Ok(skinparams(&decls, ELEMENTS)?
            .into_iter()
            .map(|p| (p.key, p.value))
            .collect())
    }

    #[test]
    fn a_declaration_sets_the_skinparams_of_what_it_reaches() {
        assert_eq!(
            params("root {\n  FontColor #333\n  FontName Liberation Sans\n  RoundCorner 4\n}")
                .unwrap(),
            own(&[
                ("TitleFontColor", "#333"),
                ("HeaderFontColor", "#333"),
                ("FooterFontColor", "#333"),
                ("CaptionFontColor", "#333"),
                ("MainframeFontColor", "#333"),
                ("ParticipantFontColor", "#333"),
                ("LegendFontColor", "#333"),
                ("defaultFontName", "Liberation Sans"),
                ("roundCorner", "4"),
            ])
        );
        assert_eq!(
            params("document { BackGroundColor none }\narrow { LineColor red }").unwrap(),
            own(&[
                ("BackgroundColor", "transparent"),
                ("LegendBackgroundColor", "transparent"),
                ("ArrowColor", "red"),
            ])
        );
        // Another diagram type's elements, and what reaches nothing here.
        assert_eq!(
            params("activityDiagram { activity { BackGroundColor red } }\nclassDiagram { .x { FontColor red } }\nroot { Shadowing 0 }")
                .unwrap(),
            []
        );
    }

    #[test]
    fn what_an_element_does_not_draw_declines_only_when_named() {
        // `root` reaches the title, which draws no box: nothing happens there.
        assert_eq!(
            params("root { BackGroundColor #222 }").unwrap(),
            own(&[
                ("BackgroundColor", "#222"),
                ("ParticipantBackgroundColor", "#222"),
                ("LegendBackgroundColor", "#222"),
            ])
        );
        for (sheet, part) in [
            (
                "title { BackGroundColor yellow }",
                "`title { BackGroundColor yellow }` is not supported",
            ),
            ("participant { Padding 4 }", "`participant { Padding 4 }`"),
            ("sequenceDiagram { FontName Courier }", "for `root` only"),
            ("root { Shadowing 3 }", "shadows"),
            ("participant { .x { FontColor red } }", "stereotypes"),
        ] {
            let err = params(sheet).unwrap_err();
            assert_eq!(err.line, Some(2), "{sheet}");
            assert!(err.reason.contains(part), "{sheet}: {}", err.reason);
        }
    }
}
