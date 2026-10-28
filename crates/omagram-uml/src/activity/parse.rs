//! From the body's lines to a [`Diagram`], the way PlantUML's activity
//! commands read them: open blocks on a stack, an arrow waiting for the
//! step it leads to, the swimlane of every step. A line the parser does
//! not understand declines the whole diagram, naming that line.

use super::words::{
    ActionEnd, action_end, action_start, backward, else_if, group, repeat_tail, squeezed,
    test_and_label, two_words, until_action_end, until_semicolon, while_tail, word,
};
use super::{
    ActionShape, Arrow, Backward, Branch, Diagram, Flow, Lane, LineStyle, Node, Note, Parallel,
    Step, Text,
};
use crate::Declined;
use crate::common::{Frame, block, keyword, snippet, text_at};
use crate::source::{Line, starts_with_ci};

/// Whether the lines of an `@startuml` diagram are an activity diagram:
/// one of them is written only there.
pub fn is_activity(lines: &[Line]) -> bool {
    lines.iter().enumerate().any(|(i, l)| {
        marks_activity(l.text) || opens_long_action(l.text, lines[i + 1..].iter().map(|l| l.text))
    })
}

/// Whether `line`, with the lines `after` it, opens a long action:
/// `:Title:` on a line of its own whose action ends with `;` on a later
/// line, not the actor shorthand of a use case diagram.
fn opens_long_action<'a>(line: &str, after: impl IntoIterator<Item = &'a str>) -> bool {
    line.starts_with(':')
        && !line.ends_with(';')
        && after
            .into_iter()
            .take_while(|next| !next.starts_with(':'))
            .any(|next| next.ends_with(';'))
}

/// `- Action` or `* Action`, PlantUML's list of actions: the marks and the
/// text. `None` for anything else, such as an arrow (`->`).
fn list_item(text: &str) -> Option<(usize, &str)> {
    let mark = text.chars().next().filter(|c| matches!(c, '-' | '*'))?;
    let depth = text.chars().take_while(|&c| c == mark).count();
    let rest = &text[depth..];
    let single_dash = mark == '-' && depth == 1;
    (rest.starts_with(char::is_whitespace)
        && !rest.trim().is_empty()
        && (mark == '*' || single_dash))
        .then(|| (depth, rest.trim()))
}

/// `:hello;` is an action. `:User:` and `:User: --> (Login)` are the actor
/// shorthand of a use case diagram, so they do not count.
fn starts_action(text: &str) -> bool {
    if !text.starts_with(':') {
        return false;
    }
    let Some(end) = text[1..].find(':') else {
        return true;
    };
    if end == 0 {
        return true;
    }
    let after = text[end + 2..].trim_start();
    if after.is_empty() || starts_with_ci(after, "as ") || after.eq_ignore_ascii_case("as") {
        return false;
    }
    // A use case line never ends like an action.
    text.ends_with(';') || !arrow_ahead(after)
}

/// Whether `text` starts with a line between two elements: at most two
/// head marks (`<|`, `*`, `o`, `x`, …) and then `-` or `.`.
fn arrow_ahead(text: &str) -> bool {
    let body = text
        .char_indices()
        .take(3)
        .find(|&(_, c)| {
            !matches!(
                c,
                '<' | '|' | '*' | 'o' | 'O' | 'x' | 'X' | '#' | '+' | '^' | '}' | '{'
            )
        })
        .map_or("", |(at, _)| &text[at..]);
    body.starts_with(['-', '.'])
}

fn marks_activity(text: &str) -> bool {
    let words = squeezed(text);
    starts_action(text)
        || list_item(text).is_some()
        || text.starts_with("(*)")
        || (text.starts_with('#') && text.contains(':'))
        || (text.starts_with('|') && !text.starts_with("||") && text[1..].contains('|'))
        || [
            "start",
            "stop",
            "kill",
            "detach",
            "endif",
            "endwhile",
            "repeat",
            "fork",
            "forkagain",
            "endfork",
            "endmerge",
            "split",
            "splitagain",
            "endsplit",
        ]
        .contains(&words.as_str())
        || ["if", "elseif", "while", "repeatwhile"]
            .iter()
            .any(|w| words.starts_with(&format!("{w}(")))
}

/// Parses an activity diagram from the lines [`crate::source::body`] gives.
pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut p = Parser {
        frame: Frame::styled(super::skin::STYLE),
        ..Parser::default()
    };
    let mut at = 0;
    while at < lines.len() {
        at = match p.frame.line(lines, at)? {
            Some(next) => next,
            None => p.line(lines, at)?,
        };
    }
    p.finish()
}

/// The deepest blocks nest; the layout goes down them recursively.
const MAX_DEPTH: usize = 100;

/// Keywords of PlantUML that Omagram does not draw yet.
const NOT_YET: [&str; 19] = [
    "package",
    "rectangle",
    "card",
    "switch",
    "case",
    "endswitch",
    "break",
    "goto",
    "label",
    "floating",
    "swimlane",
    "skin",
    "sprite",
    "hide",
    "show",
    "newpage",
    "left to right direction",
    "top to bottom direction",
    "allowmixing",
];

#[derive(Default)]
struct Parser {
    frame: Frame,
    lanes: Vec<Lane>,
    /// The swimlane new steps go to.
    lane: usize,
    root: Flow,
    /// Open blocks, the innermost last.
    blocks: Vec<Open>,
    /// `->` waiting for the step it leads to.
    pending: Option<Arrow>,
    /// Whether a step came yet: swimlanes must come first.
    any_step: bool,
}

/// A block that is open, with what it holds so far.
struct Open {
    what: Opened,
    lane: usize,
    /// The arrow that leads to the block.
    arrow: Option<Arrow>,
    line: usize,
}

enum Opened {
    If {
        branches: Vec<Branch>,
        current: Branch,
        /// After `else`: `current` is the `else` branch.
        otherwise: bool,
    },
    While {
        test: Text,
        yes: Option<Text>,
        body: Flow,
    },
    Repeat {
        body: Flow,
        backward: Option<Backward>,
    },
    Parallel {
        split: bool,
        flows: Vec<Flow>,
        current: Flow,
    },
    Group {
        /// `partition` rather than `group`.
        partition: bool,
        title: Text,
        color: Option<String>,
        flow: Flow,
    },
}

impl Open {
    fn keyword(&self) -> &'static str {
        match self.what {
            Opened::If { .. } => "if",
            Opened::While { .. } => "while",
            Opened::Repeat { .. } => "repeat",
            Opened::Parallel { split: true, .. } => "split",
            Opened::Parallel { split: false, .. } => "fork",
            Opened::Group {
                partition: true, ..
            } => "partition",
            Opened::Group { .. } => "group",
        }
    }

    fn closer(&self) -> &'static str {
        match self.what {
            Opened::If { .. } => "endif",
            Opened::While { .. } => "endwhile",
            Opened::Repeat { .. } => "repeat while",
            Opened::Parallel { split: true, .. } => "end split",
            Opened::Parallel { split: false, .. } => "end fork",
            Opened::Group { .. } => "}",
        }
    }
}

impl Parser {
    fn line(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line { no, text, .. } = lines[at];
        let words = squeezed(text);
        if let Some(Open {
            what: Opened::Repeat {
                backward: Some(_), ..
            },
            ..
        }) = self.blocks.last()
            && two_words(text, "repeat", "while").is_none()
        {
            return Err(Declined::at(
                no,
                "anything but `repeat while` after `backward` is not supported yet",
            ));
        }
        if text.starts_with('|') && !text.starts_with("||") {
            self.swimlane(text, no)?;
            return Ok(at + 1);
        }
        match words.as_str() {
            "start" => return self.add(Node::Start, no).map(|()| at + 1),
            "stop" => return self.add(Node::Stop, no).map(|()| at + 1),
            "end" => return self.add(Node::End, no).map(|()| at + 1),
            "kill" | "detach" => return self.add(Node::Detach, no).map(|()| at + 1),
            "endif" => return self.end_if(no).map(|()| at + 1),
            "repeat" => {
                self.open(
                    Opened::Repeat {
                        body: Flow::default(),
                        backward: None,
                    },
                    no,
                )?;
                return Ok(at + 1);
            }
            "fork" | "split" => {
                self.open(
                    Opened::Parallel {
                        split: words == "split",
                        flows: Vec::new(),
                        current: Flow::default(),
                    },
                    no,
                )?;
                return Ok(at + 1);
            }
            "forkagain" | "splitagain" => {
                return self.again(words == "splitagain", no).map(|()| at + 1);
            }
            "endfork" | "forkend" => return self.end_parallel(Parallel::Fork, no).map(|()| at + 1),
            "endmerge" => return self.end_parallel(Parallel::Merge, no).map(|()| at + 1),
            "endsplit" | "splitend" => {
                return self.end_parallel(Parallel::Split, no).map(|()| at + 1);
            }
            "}" | "endgroup" | "groupend" => return self.end_group(no).map(|()| at + 1),
            _ => {}
        }
        if text.starts_with("(*") {
            return Err(Declined::at(
                no,
                "the old activity syntax (`(*)`) is not supported yet",
            ));
        }
        if let Some((color, rest)) = action_start(text) {
            if connector(rest).is_some() {
                return Err(Declined::at(no, "connectors (`(A)`) are not supported yet"));
            }
            return self.action(lines, at, color, rest);
        }
        if connector(text).is_some() {
            return Err(Declined::at(no, "connectors (`(A)`) are not supported yet"));
        }
        if let Some((depth, rest)) = list_item(text) {
            if depth > 1 {
                return Err(Declined::at(
                    no,
                    "a nested list of actions is not supported yet",
                ));
            }
            let action = Node::Action {
                text: text_at(rest, no),
                color: None,
                shape: ActionShape::Plain,
                note: None,
            };
            return self.add(action, no).map(|()| at + 1);
        }
        if text.starts_with("->") || text.starts_with("-[") {
            return self.arrow(lines, at);
        }
        if let Some(rest) = word(text, "note") {
            return self.note(lines, at, rest);
        }
        if self.conditional(text, no)? {
            return Ok(at + 1);
        }
        if let Some(rest) = two_words(text, "repeat", "while") {
            return self.end_repeat(rest, no).map(|()| at + 1);
        }
        if word(text, "backward").is_some() || (text.starts_with('(') && text.contains("backward"))
        {
            return self.backward(text, no).map(|()| at + 1);
        }
        if keyword(text, "repeat").is_some() {
            return Err(Declined::at(
                no,
                "`repeat` with an action is not supported yet",
            ));
        }
        if let Some(rest) =
            two_words(text, "end", "while").or_else(|| two_words(text, "while", "end"))
        {
            return self.end_while(rest, no).map(|()| at + 1);
        }
        if let Some(rest) = word(text, "while") {
            return self.start_while(rest, no).map(|()| at + 1);
        }
        for (keyword, partition) in [("partition", true), ("group", false)] {
            if let Some(rest) = word(text, keyword) {
                return self.start_group(rest, partition, no).map(|()| at + 1);
            }
        }
        if words.starts_with("endfork") || words.starts_with("forkend") {
            return Err(Declined::at(
                no,
                "a label on `end fork` is not supported yet",
            ));
        }
        if let Some(found) = NOT_YET.iter().find(|w| {
            starts_with_ci(text, w)
                && text[w.len()..]
                    .chars()
                    .next()
                    .is_none_or(|c| !c.is_alphanumeric())
        }) {
            return Err(Declined::at(no, format!("`{found}` is not supported yet")));
        }
        Err(Declined::at(
            no,
            format!(
                "`{}` is not part of an activity diagram the native engine can draw",
                snippet(text)
            ),
        ))
    }

    fn finish(mut self) -> Result<Diagram, Declined> {
        if let Some(open) = self.blocks.last() {
            return Err(Declined::at(
                open.line,
                format!("`{}` has no `{}`", open.keyword(), open.closer()),
            ));
        }
        if self.root.steps.is_empty() {
            return Err(Declined {
                line: None,
                reason: "an activity diagram needs an activity".into(),
            });
        }
        self.root.out = self.pending.take();
        Ok(Diagram {
            lanes: self.lanes,
            flow: self.root,
            frame: self.frame,
        })
    }

    /// The flow new steps go to.
    fn flow(&mut self) -> &mut Flow {
        match self.blocks.last_mut().map(|o| &mut o.what) {
            None => &mut self.root,
            Some(Opened::If { current, .. }) => &mut current.flow,
            Some(Opened::While { body, .. } | Opened::Repeat { body, .. }) => body,
            Some(Opened::Parallel { current, .. }) => current,
            Some(Opened::Group { flow, .. }) => flow,
        }
    }

    fn add(&mut self, node: Node, no: usize) -> Result<(), Declined> {
        let arrow = self.pending.take();
        self.push(Step {
            node,
            lane: self.lane,
            arrow,
            line: no,
        });
        Ok(())
    }

    fn push(&mut self, step: Step) {
        self.any_step = true;
        self.flow().steps.push(step);
    }

    fn open(&mut self, what: Opened, no: usize) -> Result<(), Declined> {
        if self.blocks.len() >= MAX_DEPTH {
            return Err(Declined::at(
                no,
                format!("more than {MAX_DEPTH} nested blocks are not supported"),
            ));
        }
        let arrow = self.pending.take();
        self.any_step = true;
        self.blocks.push(Open {
            what,
            lane: self.lane,
            arrow,
            line: no,
        });
        Ok(())
    }

    /// Ends the flow the open block is adding to: a waiting arrow leaves it.
    fn close_flow(&mut self) {
        let out = self.pending.take();
        if out.is_some() {
            self.flow().out = out;
        }
    }

    /// Takes the innermost block, which `closer` ends; its node goes to the
    /// flow around it.
    fn close(&mut self, node: impl FnOnce(Opened) -> Node, open: Open) {
        let Open {
            what,
            lane,
            arrow,
            line,
        } = open;
        self.push(Step {
            node: node(what),
            lane,
            arrow,
            line,
        });
    }

    fn innermost(&self, keyword: &str, no: usize) -> Result<&Open, Declined> {
        match self.blocks.last() {
            Some(open) if open.keyword() == keyword => Ok(open),
            Some(open) => Err(Declined::at(
                no,
                format!("`{}` needs its `{}` first", open.keyword(), open.closer()),
            )),
            None => Err(Declined::at(no, format!("no `{keyword}` is open here"))),
        }
    }

    fn swimlane(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        let inner = &text[1..];
        let (color, inner) = match inner.strip_prefix('#') {
            Some(_) => {
                let end = inner
                    .find('|')
                    .ok_or_else(|| Declined::at(no, "a swimlane is written `|name|`"))?;
                (Some(inner[..end].to_owned()), &inner[end + 1..])
            }
            None => (None, inner),
        };
        let (name, title) = inner
            .split_once('|')
            .ok_or_else(|| Declined::at(no, "a swimlane is written `|name|`"))?;
        let (name, title) = (name.trim(), title.trim());
        if name.is_empty() || title.contains('|') {
            return Err(Declined::at(no, "a swimlane is written `|name|`"));
        }
        if self.lanes.is_empty() && self.any_step {
            return Err(Declined::at(
                no,
                "the first swimlane must come before the first activity",
            ));
        }
        let index = match self.lanes.iter().position(|l| l.name == name) {
            Some(index) => index,
            None => {
                self.lanes.push(Lane {
                    name: name.to_owned(),
                    title: text_at(name, no),
                    color: None,
                });
                self.lanes.len() - 1
            }
        };
        if let Some(open) = self.blocks.iter().find(|o| {
            matches!(
                o.what,
                Opened::While { .. } | Opened::Repeat { .. } | Opened::Group { .. }
            )
        }) && index != self.lane
        {
            return Err(Declined::at(
                no,
                format!(
                    "a swimlane change inside `{}` is not supported yet",
                    open.keyword()
                ),
            ));
        }
        let lane = &mut self.lanes[index];
        if !title.is_empty() {
            lane.title = text_at(title, no);
        }
        if color.is_some() {
            lane.color = color;
        }
        self.lane = index;
        Ok(())
    }

    /// `:text;`, or `:first` up to a line that ends the action: with `;`,
    /// `; <<shape>>`, `; <<#color>>` or an SDL mark (`:text<`).
    fn action(
        &mut self,
        lines: &[Line],
        at: usize,
        color: Option<&str>,
        rest: &str,
    ) -> Result<usize, Declined> {
        let no = lines[at].no;
        let (value, end, next) = match action_end(rest) {
            Some((text, end)) => (text.to_owned(), end, at + 1),
            None => until_action_end(lines, at, rest)?,
        };
        let mut color = color.map(str::to_owned);
        let shape = match end {
            ActionEnd::Plain => ActionShape::Plain,
            ActionEnd::Mark(mark) => shape_of_mark(mark),
            ActionEnd::Stereotype(name) if name.starts_with('#') => {
                color = Some(name);
                ActionShape::Plain
            }
            ActionEnd::Stereotype(name) => shape_named(&name).ok_or_else(|| {
                Declined::at(
                    no,
                    format!("the action shape `<<{name}>>` is not supported yet"),
                )
            })?,
        };
        self.add(
            Node::Action {
                text: text_at(&value, no),
                color,
                shape,
                note: None,
            },
            no,
        )?;
        Ok(next)
    }

    /// `->`, `-[#red,dashed]->`, with a label up to `;`.
    fn arrow(&mut self, lines: &[Line], at: usize) -> Result<usize, Declined> {
        let Line { no, text, .. } = lines[at];
        let mut arrow = Arrow {
            label: None,
            color: None,
            style: LineStyle::Plain,
            line: no,
        };
        let rest = if let Some(rest) = text.strip_prefix("->") {
            rest
        } else {
            let inner = &text[2..];
            let end = inner
                .find("]->")
                .ok_or_else(|| Declined::at(no, "an arrow is written `->` or `-[#color]->`"))?;
            for part in inner[..end].split([',', ';']).map(str::trim) {
                if part.starts_with('#') && arrow.color.is_none() {
                    arrow.color = Some(part.to_owned());
                    continue;
                }
                arrow.style = match part.to_ascii_lowercase().as_str() {
                    "dashed" => LineStyle::Dashed,
                    "dotted" => LineStyle::Dotted,
                    "bold" => LineStyle::Bold,
                    "plain" => LineStyle::Plain,
                    _ => {
                        return Err(Declined::at(
                            no,
                            format!("the arrow style `{part}` is not supported yet"),
                        ));
                    }
                };
            }
            &inner[end + 3..]
        };
        let rest = rest.trim();
        let (label, next) = if rest.is_empty() {
            (String::new(), at + 1)
        } else if let Some(label) = rest.strip_suffix(';') {
            (label.trim().to_owned(), at + 1)
        } else {
            until_semicolon(lines, at, rest, "the arrow's label")?
        };
        if !label.is_empty() {
            arrow.label = Some(text_at(&label, no));
        }
        self.pending = Some(arrow);
        Ok(next)
    }

    /// `note left: text`, `note right #color` … `end note`, after an action.
    fn note(&mut self, lines: &[Line], at: usize, rest: &str) -> Result<usize, Declined> {
        let no = lines[at].no;
        let mut rest = rest;
        let mut right = false;
        for (side, is_right) in [("left", false), ("right", true)] {
            if let Some(after) = word(rest, side) {
                right = is_right;
                rest = after;
            }
        }
        let mut color = None;
        if rest.starts_with('#') {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ':')
                .unwrap_or(rest.len());
            color = Some(rest[..end].to_owned());
            rest = rest[end..].trim_start();
        }
        let (value, next) = if let Some(text) = rest.strip_prefix(':') {
            (text.trim().to_owned(), at + 1)
        } else if rest.is_empty() {
            block(lines, at, "note", |l| {
                let w = squeezed(l);
                w == "endnote"
            })?
        } else {
            return Err(Declined::at(
                no,
                format!("`note {}` is not supported yet", snippet(rest)),
            ));
        };
        let note = Note {
            right,
            text: text_at(&value, no),
            color,
        };
        match self.flow().steps.last_mut() {
            Some(Step {
                node: Node::Action {
                    note: slot @ None, ..
                },
                ..
            }) => {
                *slot = Some(note);
                Ok(next)
            }
            _ => Err(Declined::at(
                no,
                "a note that does not follow an action is not supported yet",
            )),
        }
    }

    /// `if`, `elseif`, `else`; whether the line is one.
    fn conditional(&mut self, text: &str, no: usize) -> Result<bool, Declined> {
        if let Some(rest) = word(text, "if") {
            let (test, label) = test_and_label(rest)
                .ok_or_else(|| Declined::at(no, "an `if` is written `if (test) then (yes)`"))?;
            self.open(
                Opened::If {
                    branches: Vec::new(),
                    current: Branch {
                        test: Some(text_at(test, no)),
                        label: label.map(|l| text_at(l, no)),
                        incoming: None,
                        flow: Flow::default(),
                    },
                    otherwise: false,
                },
                no,
            )?;
            return Ok(true);
        }
        if let Some((incoming, rest)) = else_if(text) {
            let (test, label) = test_and_label(rest).ok_or_else(|| {
                Declined::at(no, "an `elseif` is written `elseif (test) then (yes)`")
            })?;
            let branch = Branch {
                test: Some(text_at(test, no)),
                label: label.map(|l| text_at(l, no)),
                incoming: incoming.map(|l| text_at(l, no)),
                flow: Flow::default(),
            };
            self.next_branch(branch, false, no)?;
            return Ok(true);
        }
        if let Some(rest) = word(text, "else") {
            let rest = rest.strip_suffix(';').unwrap_or(rest).trim();
            let label = if rest.is_empty() {
                None
            } else {
                let inner = rest
                    .strip_prefix('(')
                    .and_then(|r| r.strip_suffix(')'))
                    .ok_or_else(|| Declined::at(no, "an `else` is written `else (no)`"))?;
                Some(text_at(inner.trim(), no)).filter(|t| !t.value.is_empty())
            };
            let branch = Branch {
                label,
                ..Branch::default()
            };
            self.next_branch(branch, true, no)?;
            return Ok(true);
        }
        Ok(false)
    }

    fn next_branch(&mut self, branch: Branch, is_else: bool, no: usize) -> Result<(), Declined> {
        let keyword = if is_else { "else" } else { "elseif" };
        self.innermost("if", no)?;
        self.close_flow();
        let Some(Open {
            what:
                Opened::If {
                    branches,
                    current,
                    otherwise,
                },
            ..
        }) = self.blocks.last_mut()
        else {
            unreachable!("checked by innermost");
        };
        if *otherwise {
            return Err(Declined::at(no, format!("`{keyword}` after `else`")));
        }
        branches.push(std::mem::replace(current, branch));
        *otherwise = is_else;
        Ok(())
    }

    fn end_if(&mut self, no: usize) -> Result<(), Declined> {
        self.innermost("if", no)?;
        self.close_flow();
        let open = self.blocks.pop().expect("checked by innermost");
        self.close(
            |what| match what {
                Opened::If {
                    mut branches,
                    current,
                    otherwise,
                } => {
                    let otherwise = if otherwise {
                        current
                    } else {
                        branches.push(current);
                        Branch::default()
                    };
                    Node::If {
                        branches,
                        otherwise,
                    }
                }
                _ => unreachable!("checked by innermost"),
            },
            open,
        );
        Ok(())
    }

    fn start_while(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        let bad = || Declined::at(no, "a `while` is written `while (test) is (yes)`");
        let (test, tail) = group(rest, |tail| while_tail(tail).is_some()).ok_or_else(bad)?;
        let yes = while_tail(tail).ok_or_else(bad)?;
        self.open(
            Opened::While {
                test: text_at(test, no),
                yes: yes.map(|y| text_at(y, no)),
                body: Flow::default(),
            },
            no,
        )
    }

    fn end_while(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        let rest = rest.strip_suffix(';').unwrap_or(rest).trim();
        let out = if rest.is_empty() {
            None
        } else {
            let inner = rest
                .strip_prefix('(')
                .and_then(|r| r.strip_suffix(')'))
                .ok_or_else(|| Declined::at(no, "an `endwhile` is written `endwhile (no)`"))?;
            Some(text_at(inner, no))
        };
        self.innermost("while", no)?;
        self.close_flow();
        let open = self.blocks.pop().expect("checked by innermost");
        self.close(
            |what| match what {
                Opened::While { test, yes, body } => Node::While {
                    test,
                    yes,
                    body,
                    out,
                },
                _ => unreachable!("checked by innermost"),
            },
            open,
        );
        Ok(())
    }

    fn end_repeat(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        let bad = || {
            Declined::at(
                no,
                "a `repeat while` is written `repeat while (test) is (yes) not (no)`",
            )
        };
        let rest = rest.strip_suffix(';').unwrap_or(rest).trim();
        let (test, yes, out) = if rest.is_empty() {
            (None, None, None)
        } else {
            if rest.contains("->") {
                return Err(Declined::at(
                    no,
                    "an arrow after `repeat while` is not supported yet",
                ));
            }
            let (test, tail) = group(rest, |tail| repeat_tail(tail).is_some()).ok_or_else(bad)?;
            let (yes, out) = repeat_tail(tail).ok_or_else(bad)?;
            (Some(test).filter(|t| !t.trim().is_empty()), yes, out)
        };
        self.innermost("repeat", no)?;
        self.close_flow();
        let open = self.blocks.pop().expect("checked by innermost");
        let at = |t: &str| text_at(t, no);
        self.close(
            |what| match what {
                Opened::Repeat { body, backward } => Node::Repeat {
                    body,
                    backward,
                    test: test.map(at),
                    yes: yes.map(at),
                    out: out.map(at),
                },
                _ => unreachable!("checked by innermost"),
            },
            open,
        );
        Ok(())
    }

    /// `(in) backward :text; (out)`: the action on the way back of the
    /// `repeat` around it.
    fn backward(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        let (incoming, label, outgoing) = backward(text).ok_or_else(|| {
            Declined::at(
                no,
                if text.contains(';') {
                    "a `backward` is written `backward :text;`"
                } else {
                    "a `backward` on several lines is not supported yet"
                },
            )
        })?;
        if [incoming, outgoing]
            .iter()
            .flatten()
            .any(|l| l.starts_with(['-', '#']))
        {
            return Err(Declined::at(
                no,
                "colors on the lines of a `backward` are not supported yet",
            ));
        }
        self.innermost("repeat", no)?;
        self.close_flow();
        let at = |l: &str| Some(text_at(l, no)).filter(|t| !t.value.is_empty());
        if let Some(Open {
            what: Opened::Repeat { backward, .. },
            ..
        }) = self.blocks.last_mut()
        {
            *backward = Some(Backward {
                text: text_at(label, no),
                incoming: incoming.and_then(at),
                outgoing: outgoing.and_then(at),
            });
        }
        Ok(())
    }

    /// `partition #color "name" #color {`, `group name`: the name, quoted
    /// or not, a color before or after it, the `{` optional.
    fn start_group(&mut self, rest: &str, partition: bool, no: usize) -> Result<(), Declined> {
        let mut rest = rest.trim();
        if let Some(open) = rest.strip_suffix('{') {
            rest = open.trim_end();
        }
        if rest.contains("<<") {
            return Err(Declined::at(
                no,
                "a stereotype on a partition is not supported yet",
            ));
        }
        let mut color = None;
        if rest.starts_with('#') {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            color = Some(rest[..end].to_owned());
            rest = rest[end..].trim_start();
        } else if let Some(at) = rest.rfind(char::is_whitespace)
            && rest[at + 1..].starts_with('#')
        {
            color = Some(rest[at + 1..].to_owned());
            rest = rest[..at].trim_end();
        }
        let name = rest
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .unwrap_or(rest);
        self.open(
            Opened::Group {
                partition,
                title: text_at(name, no),
                color,
                flow: Flow::default(),
            },
            no,
        )
    }

    /// `}` or `end group`: the innermost partition or group ends. An arrow
    /// waiting at its end leads on to the step after it.
    fn end_group(&mut self, no: usize) -> Result<(), Declined> {
        let keyword = match self.blocks.last() {
            Some(Open {
                what: Opened::Group {
                    partition: true, ..
                },
                ..
            }) => "partition",
            _ => "group",
        };
        self.innermost(keyword, no)?;
        let open = self.blocks.pop().expect("checked by innermost");
        self.close(
            |what| match what {
                Opened::Group {
                    title, color, flow, ..
                } => Node::Group { title, color, flow },
                _ => unreachable!("checked by innermost"),
            },
            open,
        );
        Ok(())
    }

    fn again(&mut self, split: bool, no: usize) -> Result<(), Declined> {
        self.innermost(if split { "split" } else { "fork" }, no)?;
        self.close_flow();
        if let Some(Open {
            what: Opened::Parallel { flows, current, .. },
            ..
        }) = self.blocks.last_mut()
        {
            flows.push(std::mem::take(current));
        }
        Ok(())
    }

    fn end_parallel(&mut self, kind: Parallel, no: usize) -> Result<(), Declined> {
        self.innermost(
            if kind == Parallel::Split {
                "split"
            } else {
                "fork"
            },
            no,
        )?;
        self.close_flow();
        let open = self.blocks.pop().expect("checked by innermost");
        self.close(
            |what| match what {
                Opened::Parallel {
                    mut flows, current, ..
                } => {
                    flows.push(current);
                    Node::Parallel { kind, flows }
                }
                _ => unreachable!("checked by innermost"),
            },
            open,
        );
        Ok(())
    }
}

/// `(A)`: a connector's name.
fn connector(text: &str) -> Option<&str> {
    let name = text.trim().strip_prefix('(')?.strip_suffix(')')?;
    (!name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')).then_some(name)
}

/// The SDL shape an action's last mark names.
fn shape_of_mark(mark: char) -> ActionShape {
    match mark {
        '<' => ActionShape::Input,
        '>' => ActionShape::Output,
        '|' => ActionShape::Procedure,
        '/' => ActionShape::Save,
        '\\' => ActionShape::Load,
        '}' => ActionShape::Continuous,
        _ => ActionShape::Task,
    }
}

/// The shape `<<name>>` asks for, as PlantUML names them.
fn shape_named(name: &str) -> Option<ActionShape> {
    Some(match name.to_ascii_lowercase().as_str() {
        "input" => ActionShape::Input,
        "output" => ActionShape::Output,
        "procedure" => ActionShape::Procedure,
        "save" => ActionShape::Save,
        "load" => ActionShape::Load,
        "continuous" => ActionShape::Continuous,
        "task" => ActionShape::Task,
        "object" => ActionShape::Object,
        "acceptevent" => ActionShape::AcceptEvent,
        "sendsignal" => ActionShape::SendSignal,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::body;

    fn diagram(source: &str) -> Diagram {
        parse(&body(source).unwrap().1).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn declined(source: &str) -> Declined {
        parse(&body(source).unwrap().1).expect_err("should decline")
    }

    /// The flow as a short text: `A(text)`, `if[test?label: …|…]`, ….
    fn outline(flow: &Flow) -> String {
        let steps: Vec<String> = flow
            .steps
            .iter()
            .map(|s| {
                let arrow = match &s.arrow {
                    Some(Arrow { label: Some(l), .. }) => format!("-{}->", l.value),
                    Some(_) => "->".into(),
                    None => String::new(),
                };
                let node = match &s.node {
                    Node::Start => "start".into(),
                    Node::Stop => "stop".into(),
                    Node::End => "end".into(),
                    Node::Detach => "detach".into(),
                    Node::Action { text, note, .. } => match note {
                        Some(n) => format!("A({};note {})", text.value, n.text.value),
                        None => format!("A({})", text.value),
                    },
                    Node::If {
                        branches,
                        otherwise,
                    } => {
                        let mut parts: Vec<String> = branches
                            .iter()
                            .map(|b| {
                                format!(
                                    "{}{}?{}:{}",
                                    b.incoming
                                        .as_ref()
                                        .map_or(String::new(), |i| format!("({})", i.value)),
                                    b.test.as_ref().map_or("", |t| t.value.as_str()),
                                    b.label.as_ref().map_or("", |t| t.value.as_str()),
                                    outline(&b.flow)
                                )
                            })
                            .collect();
                        parts.push(format!(
                            "else {}:{}",
                            otherwise.label.as_ref().map_or("", |t| t.value.as_str()),
                            outline(&otherwise.flow)
                        ));
                        format!("if[{}]", parts.join("|"))
                    }
                    Node::While {
                        test,
                        yes,
                        body,
                        out,
                    } => format!(
                        "while[{}?{}:{}:{}]",
                        test.value,
                        yes.as_ref().map_or("", |t| t.value.as_str()),
                        outline(body),
                        out.as_ref().map_or("", |t| t.value.as_str())
                    ),
                    Node::Repeat {
                        body,
                        backward,
                        test,
                        yes,
                        out,
                    } => format!(
                        "repeat[{}{}:{}?{}:{}]",
                        outline(body),
                        backward
                            .as_ref()
                            .map_or(String::new(), |b| format!(" back({})", b.text.value)),
                        test.as_ref().map_or("", |t| t.value.as_str()),
                        yes.as_ref().map_or("", |t| t.value.as_str()),
                        out.as_ref().map_or("", |t| t.value.as_str())
                    ),
                    Node::Group { title, flow, .. } => {
                        format!("group[{}:{}]", title.value, outline(flow))
                    }
                    Node::Parallel { kind, flows } => format!(
                        "{kind:?}[{}]",
                        flows.iter().map(outline).collect::<Vec<_>>().join("|")
                    ),
                };
                format!("{arrow}{node}@{}", s.lane)
            })
            .collect();
        let out = match &flow.out {
            Some(Arrow { label: Some(l), .. }) => format!(" -{}->", l.value),
            Some(_) => " ->".into(),
            None => String::new(),
        };
        format!("{}{out}", steps.join(" "))
    }

    #[test]
    fn actions_and_ends() {
        let d = diagram(
            "@startuml\ntitle T\nstart\n:one;\n#pink:two;\n:three\nlines\n\nwith blank;\n:a; b;\nstop\nend\nkill\ndetach;\n@enduml",
        );
        assert_eq!(
            outline(&d.flow),
            "start@0 A(one)@0 A(two)@0 A(three\nlines\n\nwith blank)@0 A(a; b)@0 stop@0 end@0 detach@0 detach@0"
        );
        let Node::Action { color, .. } = &d.flow.steps[2].node else {
            panic!()
        };
        assert_eq!(color.as_deref(), Some("#pink"));
        assert_eq!(d.flow.steps[3].line, 5);
        assert_eq!(d.frame.title.as_ref().unwrap().value, "T");
    }

    /// The shapes and colors an action's end may give, as PlantUML's own
    /// examples write them.
    #[test]
    fn an_action_ends_with_a_shape_or_a_color() {
        let d = diagram(
            "@startuml\n:in; <<input>>\n:out; <<Output>>\n:call(o)\non two lines; <<procedure>>\n:ok; <<#palegreen>>\n:wait<\n:send>\n:p|\n:s/\n:l\\\n:c}\n:t]\n:obj; <<object>>\n:got; <<acceptEvent>>\n@enduml",
        );
        let shapes: Vec<(ActionShape, Option<String>)> = d
            .flow
            .steps
            .iter()
            .map(|s| match &s.node {
                Node::Action { shape, color, .. } => (*shape, color.clone()),
                other => panic!("{other:?}"),
            })
            .collect();
        use ActionShape::*;
        let plain = |shape| (shape, None);
        assert_eq!(
            shapes,
            [
                plain(Input),
                plain(Output),
                plain(Procedure),
                (Plain, Some("#palegreen".to_owned())),
                plain(Input),
                plain(Output),
                plain(Procedure),
                plain(Save),
                plain(Load),
                plain(Continuous),
                plain(Task),
                plain(Object),
                plain(AcceptEvent),
            ]
        );
        let Node::Action { text, .. } = &d.flow.steps[2].node else {
            panic!()
        };
        assert_eq!(text.value, "call(o)\non two lines");
        let err = declined("@startuml\n:tick; <<timeEvent>>\n@enduml");
        assert!(err.reason.contains("<<timeEvent>>"), "{}", err.reason);
        // Creole and HTML at the end of a line do not end an action.
        let d = diagram(
            "@startuml\n:a //italics//\n<b>bold</b>\nicon <&star>\nsee [[link]]\n| a | b |\ndone;\n@enduml",
        );
        assert_eq!(d.flow.steps.len(), 1);
        let err = declined("@startuml\nstart\n#blue:(B)\n@enduml");
        assert_eq!(
            (err.line, err.reason.contains("connectors")),
            (Some(2), true)
        );
    }

    /// PlantUML's lists of actions, and a long action whose first line
    /// looks like an actor.
    #[test]
    fn lists_and_long_actions() {
        let d = diagram("@startuml\n- one\n* two\n->go;\n-   three\n@enduml");
        assert_eq!(outline(&d.flow), "A(one)@0 A(two)@0 -go->A(three)@0");
        let err = declined("@startuml\n* one\n** two\n@enduml");
        assert_eq!(err.line, Some(2));
        let lines = |text: &'static str| body(text).unwrap().1;
        assert!(is_activity(&lines(
            "@startuml\n:Title:\n  more text;\n@enduml"
        )));
        assert!(is_activity(&lines("@startuml\n- one\n@enduml")));
        assert!(!is_activity(&lines(
            "@startuml\n:User: as U\n(Login)\n@enduml"
        )));
        assert!(!is_activity(&lines("@startuml\nA -> B\n--\n@enduml")));
    }

    #[test]
    fn conditions_in_every_form() {
        let d = diagram(
            "if (a (b)) then (yes)\n:x;\n(no) elseif (c) then (yes)\n:y;\nelse if (d)\nelse (other)\n:z;\nendif\nif (e) is (ok) then\nendif\nif (f)\n:w;\nelse\nend if",
        );
        assert_eq!(
            outline(&d.flow),
            "if[a (b)?yes:A(x)@0|(no)c?yes:A(y)@0|d?:|else other:A(z)@0]@0 if[e?ok:|else :]@0 if[f?:A(w)@0|else :]@0"
        );
    }

    #[test]
    fn loops_and_parallels() {
        let d = diagram(
            "while (more?) is (yes)\n:work;\nendwhile (no)\nwhile (x)\nend while\nrepeat\n:try;\nrepeat while (again?) is (yes) not (no)\nrepeat\nrepeat while\nrepeat\nrepeat while (r) not (done)\nfork\n:a;\nfork again\n:b;\nend fork\nfork\nend merge\nsplit\n:c;\nsplit again\nsplit end",
        );
        assert_eq!(
            outline(&d.flow),
            "while[more??yes:A(work)@0:no]@0 while[x?::]@0 repeat[A(try)@0:again??yes:no]@0 repeat[:?:]@0 repeat[:r?:done]@0 Fork[A(a)@0|A(b)@0]@0 Merge[]@0 Split[A(c)@0|]@0"
        );
    }

    #[test]
    fn arrows_notes_and_swimlanes() {
        let d = diagram(
            "|#AntiqueWhite|Dev|\n|QA| Quality\n|Dev|\nstart\n-> go;\n:build;\nnote right: fast\n-[#red,dashed]->\n|QA|\n:test;\nnote\n  two\n  lines\nend note\n-> first\nsecond;\nif (ok?) then (yes)\n:ship;\n-> out;\nendif\n-> done;",
        );
        assert_eq!(
            outline(&d.flow),
            "start@0 -go->A(build;note fast)@0 ->A(test;note two\nlines)@1 -first\nsecond->if[ok??yes:A(ship)@1 -out->|else :]@1 -done->"
        );
        assert_eq!(d.lanes.len(), 2);
        assert_eq!(d.lanes[0].color.as_deref(), Some("#AntiqueWhite"));
        assert_eq!(d.lanes[1].title.value, "Quality");
        let arrow = d.flow.steps[2].arrow.as_ref().unwrap();
        assert_eq!(
            (arrow.color.as_deref(), arrow.style),
            (Some("#red"), LineStyle::Dashed)
        );
        let Node::Action {
            note: Some(note), ..
        } = &d.flow.steps[1].node
        else {
            panic!()
        };
        assert!(note.right);
    }

    #[test]
    fn backward_in_repeat() {
        let d = diagram(
            "repeat\n:try;\nbackward :wait;\nrepeat while (failed?)\nrepeat\n(late) backward:retry; (again)\nrepeat while",
        );
        assert_eq!(
            outline(&d.flow),
            "repeat[A(try)@0 back(wait):failed??:]@0 repeat[ back(retry):?:]@0"
        );
        let Node::Repeat {
            backward: Some(b), ..
        } = &d.flow.steps[1].node
        else {
            panic!()
        };
        let labels = (b.incoming.as_ref(), b.outgoing.as_ref());
        assert_eq!(
            (
                labels.0.unwrap().value.as_str(),
                labels.1.unwrap().value.as_str()
            ),
            ("late", "again")
        );
    }

    #[test]
    fn partitions_and_groups() {
        let d = diagram(
            "partition Order {\n:take;\n}\npartition #pink \"Two words\" {\n}\npartition Late #aqua\n:x;\nend group\ngroup g\npartition inner {\n:deep;\n}\nend group",
        );
        let groups: Vec<(String, Option<String>, usize)> = d
            .flow
            .steps
            .iter()
            .map(|s| match &s.node {
                Node::Group { title, color, flow } => {
                    (title.value.clone(), color.clone(), flow.steps.len())
                }
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            groups,
            [
                ("Order".into(), None, 1),
                ("Two words".into(), Some("#pink".into()), 0),
                ("Late".into(), Some("#aqua".into()), 1),
                ("g".into(), None, 1),
            ]
        );
    }

    #[test]
    fn tells_activity_from_sequence() {
        for (source, activity) in [
            ("A -> B\nB --> A", false),
            ("participant X\n|||\nX -> X", false),
            (":hello;", true),
            (":hello: more;", true),
            (":User:", false),
            (":User: as Clerk", false),
            (":User: --> (Login)", false),
            ("start\nA -> B", true),
            ("|Lane|\n:x;", true),
            ("if(x)\nendif", true),
            ("#red:x;", true),
        ] {
            let lines = body(source).unwrap().1;
            assert_eq!(is_activity(&lines), activity, "{source}");
        }
    }

    #[test]
    fn declines_what_it_does_not_draw() {
        let deep = "if (x)\n".repeat(101);
        for (source, line, part) in [
            ("start\nendif", 1, "no `if` is open"),
            ("if (a)\n:x;", 0, "`if` has no `endif`"),
            ("if (a)\nelse\nelse\nendif", 2, "`else` after `else`"),
            ("if (a)\nfork\nendif", 2, "`fork` needs its `end fork`"),
            ("while (a)\nrepeat while", 1, "`while` needs its `endwhile`"),
            (":x;\n|A|", 1, "first swimlane"),
            ("|A|\nwhile (x)\n|B|\nendwhile", 2, "inside `while`"),
            ("start\nnote right: x", 1, "follow an action"),
            (":x; <<trigger>>", 0, "`<<trigger>>`"),
            (":x\ny", 0, "`;`"),
            ("start\npackage P {\n}", 1, "`package`"),
            ("start\n}", 1, "no `group` is open"),
            ("partition P {\nif (x)\n}", 2, "`if` needs its `endif`"),
            ("|A|\npartition P {\n|B|\n}", 2, "inside `partition`"),
            ("partition P <<s>> {\n}", 0, "stereotype"),
            ("start\nbackward :x;", 1, "no `repeat` is open"),
            (
                "repeat\nbackward :x;\n:y;\nrepeat while",
                2,
                "after `backward`",
            ),
            (
                "repeat\n(-[#red]-> a) backward :x;\nrepeat while",
                1,
                "colors",
            ),
            ("repeat\nbackward :x\nrepeat while", 1, "several lines"),
            ("repeat\nbackward x;\nrepeat while", 1, "`backward :text;`"),
            ("(*) --> \"x\"", 0, "old activity syntax"),
            ("start\n-[hidden]->\nstop", 1, "`hidden`"),
            ("fork\nend fork {and}", 1, "label on `end fork`"),
            ("repeat :x;\nrepeat while", 0, "`repeat` with an action"),
            ("start\nfoo bar", 1, "foo bar"),
            (deep.as_str(), 100, "nested"),
        ] {
            let err = declined(source);
            assert_eq!(err.line, Some(line), "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }
}
