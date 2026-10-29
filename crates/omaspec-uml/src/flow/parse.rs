//! From the body's lines to a [`Diagram`]. A wire may create a block or a
//! port the file never declared; a later `block` of that name still declines,
//! so blocks are declared before the wires that use them, or not at all.
//!
//! A block's body may hold further blocks and wires. That block is a
//! subsystem: a bare name inside it is one of its own ports, and an inner
//! block's name stays inside it.

use super::{Block, Diagram, End, Port, Wire};
use crate::Declined;
use crate::common::{Frame, keyword, snippet, squeezed_eq};
use crate::source::Line;

/// Blocks inside each other; the drawing goes down them a frame a level.
const MAX_DEPTH: usize = 100;

pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut builder = Builder::new();
    let mut at = 0;
    while at < lines.len() {
        // A `{` on the line after `block Name` opens the body. Any other
        // line means the block declares no ports, and that line belongs to
        // the block around it.
        if builder.place() == Place::Brace {
            match lines[at].text {
                "{" => {
                    builder.scope().place = Place::Body;
                    at += 1;
                    continue;
                }
                "{}" => {
                    builder.close_scope();
                    at += 1;
                    continue;
                }
                _ => builder.close_scope(),
            }
        }
        if builder.place() == Place::Body {
            builder.in_body(lines[at].text, lines[at].no)?;
            at += 1;
            continue;
        }
        if let Some(next) = builder.frame.line(lines, at)? {
            at = next;
            continue;
        }
        builder.statement(lines[at].text, lines[at].no)?;
        at += 1;
    }
    builder.finish()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Place {
    Top,
    /// The next line may be the `{` of the block just declared.
    Brace,
    Body,
}

/// One open `block`, or the diagram itself at the bottom of the stack.
struct Scope {
    name: String,
    label: String,
    line: usize,
    inputs: Vec<Port>,
    outputs: Vec<Port>,
    blocks: Vec<Block>,
    wires: Vec<Wire>,
    place: Place,
}

struct Builder {
    stack: Vec<Scope>,
    frame: Frame,
}

impl Builder {
    fn new() -> Self {
        Self {
            stack: vec![Scope {
                name: String::new(),
                label: String::new(),
                line: 0,
                inputs: Vec::new(),
                outputs: Vec::new(),
                blocks: Vec::new(),
                wires: Vec::new(),
                place: Place::Top,
            }],
            frame: Frame::default(),
        }
    }

    fn scope(&mut self) -> &mut Scope {
        self.stack.last_mut().expect("a scope is open")
    }

    fn place(&self) -> Place {
        self.stack.last().expect("a scope is open").place
    }

    fn statement(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        if !text.contains("->") && text.to_ascii_lowercase().contains("direction") {
            if squeezed_eq(text, "lefttorightdirection") {
                return Ok(());
            }
            return Err(fail(no, "only `left to right direction` is supported yet"));
        }
        if let Some(rest) = keyword(text, "block") {
            return self.block(rest, no);
        }
        if let Some(rest) = keyword(text, "in") {
            return self.boundary(rest, no, true);
        }
        if let Some(rest) = keyword(text, "out") {
            return self.boundary(rest, no, false);
        }
        if text.contains("->") {
            return self.wire(text, no);
        }
        Err(unknown(text, no))
    }

    fn in_body(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        if text == "}" {
            self.close_scope();
            return Ok(());
        }
        if let Some(rest) = keyword(text, "in") {
            return self.port(rest, no, true);
        }
        if let Some(rest) = keyword(text, "out") {
            return self.port(rest, no, false);
        }
        // A nested block, or a wire among the blocks inside this one.
        self.statement(text, no)
    }

    fn block(&mut self, rest: &str, no: usize) -> Result<(), Declined> {
        // The diagram itself is at the bottom of the stack.
        if self.stack.len() > MAX_DEPTH {
            return Err(fail(no, "blocks nest more than 100 deep"));
        }
        let rest = rest.trim();
        let (name, rest) = ident(rest).ok_or_else(|| fail(no, "`block` needs a name"))?;
        self.fresh_block(name, no)?;
        let rest = rest.trim_start();
        let (label, rest) = if rest.starts_with('"') {
            quoted(rest, no)?
        } else {
            (name.to_owned(), rest)
        };
        let name = name.to_owned();
        match brace_of(rest) {
            Brace::Bad if rest.trim_start().starts_with('{') => {
                Err(fail(no, "a block's ports go on their own lines"))
            }
            Brace::Bad => Err(unknown(rest.trim(), no)),
            Brace::None => {
                self.open_scope(name, label, no, Place::Brace);
                Ok(())
            }
            Brace::Empty => {
                self.open_scope(name, label, no, Place::Top);
                self.close_scope();
                Ok(())
            }
            Brace::Open => {
                self.open_scope(name, label, no, Place::Body);
                Ok(())
            }
        }
    }

    fn fresh_block(&self, name: &str, no: usize) -> Result<(), Declined> {
        let scope = self.stack.last().expect("a scope is open");
        if scope.blocks.iter().any(|b| b.name == name) {
            return Err(fail(no, format!("`{name}` is already a block")));
        }
        if scope.inputs.iter().any(|p| p.name == name)
            || scope.outputs.iter().any(|p| p.name == name)
        {
            return Err(fail(no, format!("`{name}` is already a signal")));
        }
        Ok(())
    }

    fn open_scope(&mut self, name: String, label: String, no: usize, place: Place) {
        self.stack.push(Scope {
            name,
            label,
            line: no,
            inputs: Vec::new(),
            outputs: Vec::new(),
            blocks: Vec::new(),
            wires: Vec::new(),
            place,
        });
    }

    /// The block just opened becomes a block of the scope around it.
    fn close_scope(&mut self) {
        let scope = self.stack.pop().expect("a block is open");
        self.scope().blocks.push(Block {
            name: scope.name,
            label: scope.label,
            line: scope.line,
            inputs: scope.inputs,
            outputs: scope.outputs,
            blocks: scope.blocks,
            wires: scope.wires,
        });
    }

    fn boundary(&mut self, rest: &str, no: usize, input: bool) -> Result<(), Declined> {
        let name = port_name(rest, no, input)?;
        if self.scope().blocks.iter().any(|b| b.name == name) {
            return Err(fail(no, format!("`{name}` is already a block")));
        }
        if self.is_signal(&name) {
            return Err(fail(no, format!("`{name}` is already a signal")));
        }
        let port = Port { name, line: no };
        if input {
            self.scope().inputs.push(port);
        } else {
            self.scope().outputs.push(port);
        }
        Ok(())
    }

    fn port(&mut self, rest: &str, no: usize, input: bool) -> Result<(), Declined> {
        let name = port_name(rest, no, input)?;
        let block = self.scope();
        if block.inputs.iter().any(|p| p.name == name)
            || block.outputs.iter().any(|p| p.name == name)
        {
            return Err(fail(
                no,
                format!("`{name}` is already a port of `{}`", block.name),
            ));
        }
        let port = Port { name, line: no };
        if input {
            block.inputs.push(port);
        } else {
            block.outputs.push(port);
        }
        Ok(())
    }

    fn wire(&mut self, text: &str, no: usize) -> Result<(), Declined> {
        let (from, to) = match arrow_ends(text) {
            Ok(ends) => ends,
            Err(ArrowError::Many) => return Err(fail(no, "a wire has one arrow")),
            Err(ArrowError::None) => return Err(unknown(text, no)),
        };
        let from = self.resolve(raw_end(from, no)?, Side::Source, no)?;
        let to = self.resolve(raw_end(to, no)?, Side::Target, no)?;
        if self.scope().wires.iter().any(|wire| wire.to == to) {
            return Err(fail(no, format!("`{}` already has a signal", show(&to))));
        }
        self.scope().wires.push(Wire {
            from,
            to,
            label: label_of(text),
            line: no,
        });
        Ok(())
    }

    fn resolve(&mut self, raw: Raw, side: Side, no: usize) -> Result<End, Declined> {
        match raw {
            Raw::Bare(name) => self.resolve_bare(name, side, no),
            Raw::Port { block, port } => self.resolve_port(block, port, side, no),
        }
    }

    fn resolve_bare(&mut self, name: String, side: Side, no: usize) -> Result<End, Declined> {
        if self.scope().blocks.iter().any(|b| b.name == name) {
            return Err(fail(
                no,
                format!("`{name}` is a block; name one of its ports, as in `{name}.out`"),
            ));
        }
        let input = self.scope().inputs.iter().any(|p| p.name == name);
        let output = self.scope().outputs.iter().any(|p| p.name == name);
        match side {
            Side::Source => {
                if output {
                    return Err(fail(no, format!("`{name}` is an output")));
                }
                if !input {
                    self.scope().inputs.push(Port {
                        name: name.clone(),
                        line: no,
                    });
                }
            }
            Side::Target => {
                if input {
                    return Err(fail(no, format!("`{name}` is an input")));
                }
                if !output {
                    self.scope().outputs.push(Port {
                        name: name.clone(),
                        line: no,
                    });
                }
            }
        }
        Ok(End::Boundary(name))
    }

    fn resolve_port(
        &mut self,
        block: String,
        port: String,
        side: Side,
        no: usize,
    ) -> Result<End, Declined> {
        let index = self.ensure_block(&block, no)?;
        let found = &mut self.scope().blocks[index];
        let input = found.inputs.iter().any(|p| p.name == port);
        let output = found.outputs.iter().any(|p| p.name == port);
        match side {
            Side::Source => {
                if input {
                    return Err(fail(no, format!("`{block}.{port}` is an input")));
                }
                if !output {
                    found.outputs.push(Port {
                        name: port.clone(),
                        line: no,
                    });
                }
            }
            Side::Target => {
                if output {
                    return Err(fail(no, format!("`{block}.{port}` is an output")));
                }
                if !input {
                    found.inputs.push(Port {
                        name: port.clone(),
                        line: no,
                    });
                }
            }
        }
        Ok(End::Port { block, port })
    }

    fn ensure_block(&mut self, name: &str, no: usize) -> Result<usize, Declined> {
        if let Some(index) = self.scope().blocks.iter().position(|b| b.name == name) {
            return Ok(index);
        }
        if self.is_signal(name) {
            return Err(fail(no, format!("`{name}` is already a signal")));
        }
        self.open_scope(name.to_owned(), name.to_owned(), no, Place::Top);
        self.close_scope();
        Ok(self.scope().blocks.len() - 1)
    }

    fn is_signal(&self, name: &str) -> bool {
        let scope = self.stack.last().expect("a scope is open");
        scope.inputs.iter().any(|p| p.name == name) || scope.outputs.iter().any(|p| p.name == name)
    }

    fn finish(mut self) -> Result<Diagram, Declined> {
        while self.stack.len() > 1 {
            match self.place() {
                Place::Body => {
                    let line = self.stack.last().expect("a scope is open").line;
                    return Err(fail(line, "the block has no `}`"));
                }
                // The `{` never came, so the block has no body.
                Place::Brace | Place::Top => self.close_scope(),
            }
        }
        let root = self.stack.pop().expect("the diagram scope");
        Ok(Diagram {
            inputs: root.inputs,
            outputs: root.outputs,
            blocks: root.blocks,
            wires: root.wires,
            frame: self.frame,
        })
    }
}

enum Side {
    Source,
    Target,
}

enum Raw {
    Bare(String),
    Port { block: String, port: String },
}

enum Brace {
    None,
    Open,
    Empty,
    Bad,
}

enum ArrowError {
    None,
    Many,
}

fn brace_of(rest: &str) -> Brace {
    let rest = rest.trim_start();
    if rest.is_empty() {
        return Brace::None;
    }
    let Some(inner) = rest.strip_prefix('{') else {
        return Brace::Bad;
    };
    match inner.trim() {
        "" => Brace::Open,
        "}" => Brace::Empty,
        _ => Brace::Bad,
    }
}

fn port_name(rest: &str, no: usize, input: bool) -> Result<String, Declined> {
    let rest = rest.trim();
    let (name, tail) = ident(rest).ok_or_else(|| {
        fail(
            no,
            if input {
                "`in` needs a name"
            } else {
                "`out` needs a name"
            },
        )
    })?;
    if !tail.trim().is_empty() {
        return Err(unknown(tail.trim(), no));
    }
    Ok(name.to_owned())
}

/// A name and the text after it. A name starts with a letter or `_`.
fn ident(text: &str) -> Option<(&str, &str)> {
    let mut chars = text.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_') {
        return None;
    }
    let mut end = first.len_utf8();
    for c in chars {
        if c.is_ascii_alphanumeric() || c == '_' {
            end += c.len_utf8();
        } else {
            break;
        }
    }
    Some((&text[..end], &text[end..]))
}

fn ident_exact(text: &str) -> bool {
    ident(text).is_some_and(|(_, rest)| rest.is_empty())
}

fn quoted(text: &str, no: usize) -> Result<(String, &str), Declined> {
    let rest = text
        .strip_prefix('"')
        .ok_or_else(|| fail(no, "a label needs a closing `\"`"))?;
    let end = rest
        .find('"')
        .ok_or_else(|| fail(no, "a label needs a closing `\"`"))?;
    Ok((rest[..end].to_owned(), rest[end + 1..].trim_start()))
}

fn raw_end(text: &str, no: usize) -> Result<Raw, Declined> {
    if text.is_empty() {
        return Err(fail(no, "a wire needs a port"));
    }
    if let Some((block, port)) = text.split_once('.') {
        let block = block.trim();
        let port = port.trim();
        if ident_exact(block) && ident_exact(port) {
            return Ok(Raw::Port {
                block: block.to_owned(),
                port: port.to_owned(),
            });
        }
        return Err(fail(no, format!("`{}` is not a port", snippet(text))));
    }
    if !ident_exact(text) {
        return Err(fail(no, format!("`{}` is not a port", snippet(text))));
    }
    Ok(Raw::Bare(text.to_owned()))
}

/// The two ends of the one `->` or `-->` before an unquoted `:`.
fn arrow_ends(text: &str) -> Result<(&str, &str), ArrowError> {
    let (head, _) = split_unquoted(text, ':');
    let mut quote = false;
    let mut found = None;
    let chars: Vec<(usize, char)> = head.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (at, c) = chars[i];
        if c == '"' {
            quote = !quote;
            i += 1;
            continue;
        }
        if !quote && c == '-' {
            let rest = &head[at..];
            let len = if rest.starts_with("-->") {
                3
            } else if rest.starts_with("->") {
                2
            } else {
                0
            };
            if len > 0 {
                if found.is_some() {
                    return Err(ArrowError::Many);
                }
                found = Some((at, len));
                i += len;
                continue;
            }
        }
        i += 1;
    }
    let Some((at, len)) = found else {
        return Err(ArrowError::None);
    };
    Ok((head[..at].trim(), head[at + len..].trim()))
}

fn label_of(text: &str) -> Option<String> {
    let (_, label) = split_unquoted(text, ':');
    let label = label?.trim();
    (!label.is_empty()).then(|| label.to_owned())
}

fn split_unquoted(text: &str, sep: char) -> (&str, Option<&str>) {
    let mut quote = false;
    for (i, c) in text.char_indices() {
        if c == '"' {
            quote = !quote;
        } else if c == sep && !quote {
            return (text[..i].trim(), Some(&text[i + sep.len_utf8()..]));
        }
    }
    (text.trim(), None)
}

fn show(end: &End) -> String {
    match end {
        End::Boundary(name) => name.clone(),
        End::Port { block, port } => format!("{block}.{port}"),
    }
}

fn unknown(text: &str, no: usize) -> Declined {
    fail(no, format!("`{}` is not supported yet", snippet(text)))
}

fn fail(line: usize, reason: impl Into<String>) -> Declined {
    Declined::at(line, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(body: &str) -> Result<Diagram, Declined> {
        let source = format!("@startflow\n{body}\n@endflow\n");
        match crate::parse(&source) {
            Ok(crate::Diagram::Flow(diagram)) => Ok(diagram),
            Ok(other) => panic!("not a signal flow: {other:?}"),
            Err(err) => Err(err),
        }
    }

    #[test]
    fn blocks_nest_at_most_a_hundred_deep() {
        let nest = |n: usize| format!("{}in a\n{}", "block B {\n".repeat(n), "}\n".repeat(n));
        // A hundred levels draw, even on a test thread's small stack.
        assert!(crate::render(&format!("@startflow\n{}@endflow\n", nest(100))).is_ok());
        let err = parsed(&nest(101)).unwrap_err();
        assert_eq!(err.line, Some(101));
        assert!(err.reason.contains("100"), "{}", err.reason);
        // Far deeper declines too, instead of overflowing the stack.
        assert!(parsed(&nest(100_000)).is_err());
    }

    fn names(ports: &[Port]) -> Vec<&str> {
        ports.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn a_chain_keeps_blocks_ports_and_the_label() {
        let diagram = parsed(
            "title Charge control\n\
             in voltage\n\
             out current\n\
             block Limit \"Limit\" {\n\
             in voltage\n\
             out limited\n\
             }\n\
             block Measure {\n\
             in limited\n\
             out current\n\
             }\n\
             voltage -> Limit.voltage\n\
             Limit.limited --> Measure.limited : a: b\n\
             Measure.current -> current\n",
        )
        .unwrap();
        assert_eq!(names(&diagram.inputs), ["voltage"]);
        assert_eq!(names(&diagram.outputs), ["current"]);
        assert_eq!(diagram.blocks[0].label, "Limit");
        assert_eq!(names(&diagram.blocks[0].inputs), ["voltage"]);
        assert_eq!(names(&diagram.blocks[0].outputs), ["limited"]);
        assert_eq!(names(&diagram.blocks[1].inputs), ["limited"]);
        assert_eq!(diagram.wires[1].label.as_deref(), Some("a: b"));
        assert_eq!(diagram.frame.title.unwrap().value, "Charge control");
    }

    #[test]
    fn a_wire_creates_ports_in_the_order_it_sees_them() {
        let diagram = parsed("A.z -> B.a\nA.m -> C.n\n").unwrap();
        assert_eq!(names(&diagram.blocks[0].outputs), ["z", "m"]);
        assert_eq!(diagram.inputs.len(), 0);
        assert_eq!(names(&diagram.outputs), Vec::<&str>::new());
    }

    #[test]
    fn a_bare_wire_is_boundary_to_boundary() {
        let diagram = parsed("a -> b\n").unwrap();
        assert_eq!(names(&diagram.inputs), ["a"]);
        assert_eq!(names(&diagram.outputs), ["b"]);
        assert!(diagram.blocks.is_empty());
    }

    #[test]
    fn a_brace_on_the_next_line_opens_the_block() {
        let diagram = parsed("block Limit\n{\nin a\n}\n").unwrap();
        assert_eq!(names(&diagram.blocks[0].inputs), ["a"]);
    }

    #[test]
    fn declines_what_it_cannot_read() {
        let err = |body: &str| parsed(body).unwrap_err();
        let unknown = err("foo bar\n");
        assert_eq!(unknown.line, Some(1));
        assert!(
            unknown.reason.contains("not supported"),
            "{}",
            unknown.reason
        );

        let direction = err("top to bottom direction\n");
        assert!(
            direction.reason.contains("left to right direction"),
            "{}",
            direction.reason
        );

        let open = err("block Limit {\nin a\n");
        assert_eq!(open.line, Some(1));
        assert!(open.reason.contains("no `}`"), "{}", open.reason);

        let both = err("block Limit {\nout a\nin a\n}\n");
        assert!(both.reason.contains("already a port"), "{}", both.reason);

        let side = err("block Limit {\nout a\n}\nLimit.a -> Limit.a\n");
        assert_eq!(side.line, Some(4));
        assert!(side.reason.contains("is an output"), "{}", side.reason);

        let twice = err("a -> Limit.x\nb -> Limit.x\n");
        assert!(
            twice.reason.contains("`Limit.x` already has a signal"),
            "{}",
            twice.reason
        );

        let again = err("A.x -> B.y\nblock A\n");
        assert!(again.reason.contains("already a block"), "{}", again.reason);

        let bare = err("block Limit\nLimit -> out\n");
        assert!(
            bare.reason.contains("name one of its ports"),
            "{}",
            bare.reason
        );

        let nested = err("block A {\nblock B {\nin x\n");
        assert_eq!(nested.line, Some(2));
        assert!(nested.reason.contains("no `}`"), "{}", nested.reason);
    }

    #[test]
    fn a_subsystem_keeps_its_blocks_and_its_ports() {
        let diagram = parsed(
            "block Charger \"Charger\" {\n\
             in voltage\n\
             out current\n\
             block Limit {\n\
             in voltage\n\
             out limited\n\
             }\n\
             voltage -> Limit.voltage\n\
             Limit.limited -> current\n\
             }\n\
             voltage -> Charger.voltage\n\
             Charger.current -> current\n",
        )
        .unwrap();
        assert_eq!(names(&diagram.inputs), ["voltage"]);
        assert_eq!(names(&diagram.outputs), ["current"]);
        assert_eq!(diagram.blocks.len(), 1);
        let charger = &diagram.blocks[0];
        assert_eq!(charger.label, "Charger");
        assert_eq!(names(&charger.inputs), ["voltage"]);
        assert_eq!(names(&charger.outputs), ["current"]);
        assert_eq!(charger.blocks.len(), 1);
        assert_eq!(charger.blocks[0].name, "Limit");
        assert_eq!(names(&charger.blocks[0].outputs), ["limited"]);
        assert_eq!(charger.wires.len(), 2);
        assert_eq!(diagram.wires.len(), 2);
        // An inner block's name is not a block of the diagram.
        assert!(diagram.blocks.iter().all(|b| b.name != "Limit"));
    }
}
