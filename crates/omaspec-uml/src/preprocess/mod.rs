//! PlantUML's preprocessor, run on the body's lines before a parser sees
//! them: variables and expressions, `!if` and `!ifdef`, the old `!define`
//! macros, functions and procedures, `!while`, and the `%` built-ins that
//! need nothing from outside. It does no I/O: `!include` of a URL, every
//! `!theme` but `plain`, and the built-ins that read the clock, the
//! environment or files decline, naming the line. `!theme plain` expands
//! to the part of that theme the engines draw.
//!
//! A diagram without a `!` line and without a `%` built-in passes through
//! untouched. A line that a procedure or a macro writes carries the line of
//! the call, so a construct the parser declines there names the call.

mod builtin;
mod expr;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use expr::{Arg, Expr, Op, Value};

use crate::Declined;
use crate::source::{self, Line};

/// Calls, `!if` and `!while` inside each other, and `!define`s inside
/// `!define`s.
const MAX_DEPTH: usize = 64;
/// Expressions evaluating inside each other, across calls: brackets around
/// a call that calls again add up, and each level is a frame of the stack.
/// A chain of operators runs in a loop and takes one level.
const MAX_EVAL: usize = 500;
/// Lines run, loops and calls included.
const MAX_STEPS: usize = 200_000;
/// Lines written.
const MAX_LINES: usize = 100_000;
/// Bytes in one written line or value.
const MAX_TEXT: usize = 1 << 20;
/// Files a diagram includes, and their bytes together.
const MAX_INCLUDES: usize = 256;
const MAX_INCLUDED: usize = 8 << 20;

/// Reads the files `!include` names. Omaspec's renderer gives one that
/// reads local files inside the project folder; without one, `!include`
/// declines.
pub trait Includes {
    /// The file `path` names, as written after `!include`, relative to the
    /// file `from` (by its key) or, for `None`, to the diagram itself. The
    /// error says why it cannot be read.
    fn read(&self, path: &str, from: Option<&str>) -> Result<Included, String>;
}

/// A file read for `!include`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Included {
    /// Names the file whichever way it was written, for "only once".
    pub key: String,
    pub text: String,
}

/// A line after preprocessing; [`Owned::line`] lends it to a parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owned {
    pub no: usize,
    pub text: String,
    pub indent: usize,
    pub blank_before: usize,
}

impl From<Line<'_>> for Owned {
    fn from(line: Line<'_>) -> Self {
        Owned {
            no: line.no,
            text: line.text.to_owned(),
            indent: line.indent,
            blank_before: line.blank_before,
        }
    }
}

impl Owned {
    pub fn line(&self) -> Line<'_> {
        Line {
            no: self.no,
            text: &self.text,
            indent: self.indent,
            blank_before: self.blank_before,
        }
    }
}

/// The lines a run was given, and what came of them.
type Kept = (Vec<Owned>, Result<Vec<Owned>, Declined>);

thread_local! {
    /// The last run without includes on this thread. Lint, outline, the
    /// checks and the drawing each read the same diagram, and a long
    /// `!while` would cost every one of them.
    static LAST: RefCell<Option<Kept>> = const { RefCell::new(None) };
}

/// Runs the preprocessor on `lines`, reading `!include`s through
/// `includes`; `None` when there is nothing for it to do, so the lines stay
/// as they are.
pub fn run(
    lines: &[Line],
    includes: Option<&dyn Includes>,
) -> Result<Option<Vec<Owned>>, Declined> {
    if !lines
        .iter()
        .any(|line| line.text.starts_with('!') || calls_builtin(line.text))
    {
        return Ok(None);
    }
    // What a file holds may change between runs; the text alone may not.
    if includes.is_some() {
        return expand(lines, includes).map(Some);
    }
    let same = |kept: &[Owned]| {
        kept.len() == lines.len() && kept.iter().zip(lines).all(|(k, l)| k.line() == *l)
    };
    let kept = LAST.with_borrow(|last| {
        last.as_ref()
            .filter(|(kept, _)| same(kept))
            .map(|(_, result)| result.clone())
    });
    if let Some(result) = kept {
        return result.map(Some);
    }
    let result = expand(lines, None);
    let key = lines.iter().copied().map(Owned::from).collect();
    LAST.set(Some((key, result.clone())));
    result.map(Some)
}

fn expand(lines: &[Line], includes: Option<&dyn Includes>) -> Result<Vec<Owned>, Declined> {
    let mut machine = Machine {
        includes,
        ..Machine::default()
    };
    let mut out = Out::default();
    machine.exec(lines, &mut Frame::default(), &mut out)?;
    Ok(out.lines)
}

/// Whether `text` has a `%name(`, which the preprocessor runs.
fn calls_builtin(text: &str) -> bool {
    text.match_indices('%').any(|(at, _)| {
        let rest = &text[at + 1..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        rest.starts_with(|c: char| c.is_ascii_alphabetic()) && rest[end..].starts_with('(')
    })
}

#[derive(Default)]
struct Machine<'i> {
    globals: HashMap<String, Value>,
    callables: HashMap<String, Rc<Callable>>,
    defines: HashMap<String, Rc<Define>>,
    steps: usize,
    depth: usize,
    evals: usize,
    includes: Option<&'i dyn Includes>,
    /// The keys of the files included so far.
    included: HashSet<String>,
    included_bytes: usize,
    /// The keys of the files being included, the innermost last.
    files: Vec<String>,
}

/// A `!function` or `!procedure`.
struct Callable {
    procedure: bool,
    /// `!unquoted`: arguments written in a line are text, not expressions.
    unquoted: bool,
    params: Vec<Param>,
    body: Body,
}

enum Body {
    Lines(Vec<Owned>),
    /// `!function $f($a) !return …` on one line: the expression and its line.
    Return(String, usize),
}

struct Param {
    name: String,
    /// The default as written: an expression, or text for a `!define`.
    default: Option<String>,
}

/// An old `!define` or `!definelong`; its text replaces its name.
struct Define {
    params: Option<Vec<Param>>,
    body: String,
}

#[derive(Default)]
struct Frame {
    /// `None` outside functions and procedures, where variables are global.
    locals: Option<HashMap<String, Value>>,
    /// Inside a function, which returns a value and writes no lines.
    function: bool,
    /// The line of the outermost call, which the written lines carry.
    call: Option<usize>,
}

enum Flow {
    Next,
    Return(Value),
}

#[derive(Default)]
struct Out {
    lines: Vec<Owned>,
    /// Blank lines not yet given to a written line.
    blanks: usize,
}

impl Out {
    /// Writes `text` for `line`; a `%newline()` in it starts another line.
    fn write(&mut self, line: Line, call: Option<usize>, text: &str) -> Result<(), Declined> {
        let mut blanks = std::mem::take(&mut self.blanks) + line.blank_before;
        for part in text.split('\n') {
            let part = part.trim();
            if part.is_empty() {
                blanks += 1;
                continue;
            }
            if self.lines.len() >= MAX_LINES {
                return Err(fail(line.no, "the preprocessor writes too many lines"));
            }
            self.lines.push(Owned {
                no: call.unwrap_or(line.no),
                text: part.to_owned(),
                indent: line.indent,
                blank_before: std::mem::take(&mut blanks),
            });
        }
        self.blanks = blanks;
        Ok(())
    }
}

impl Machine<'_> {
    fn exec(&mut self, body: &[Line], frame: &mut Frame, out: &mut Out) -> Result<Flow, Declined> {
        let mut at = 0;
        while at < body.len() {
            let line = body[at];
            self.step(line.no)?;
            let Some(directive) = line.text.strip_prefix('!') else {
                self.text_line(line, frame, out)?;
                at += 1;
                continue;
            };
            out.blanks += line.blank_before;
            let (word, rest) = split_word(directive);
            match word.clone().as_str() {
                "if" | "ifdef" | "ifndef" => {
                    self.enter(line.no)?;
                    let (flow, next) = self.branch(body, at, frame, out)?;
                    self.depth -= 1;
                    if let Flow::Return(value) = flow {
                        return Ok(Flow::Return(value));
                    }
                    at = next;
                    continue;
                }
                "while" => {
                    let end = block_end(body, at, "while", "endwhile")?;
                    self.enter(line.no)?;
                    while self.eval_text(rest, line.no, frame)?.truthy() {
                        self.step(line.no)?;
                        if let Flow::Return(value) = self.exec(&body[at + 1..end], frame, out)? {
                            return Ok(Flow::Return(value));
                        }
                    }
                    self.depth -= 1;
                    at = end + 1;
                    continue;
                }
                "function" | "procedure" | "unquoted" | "final" => {
                    at = self.define_callable(body, at)?;
                    continue;
                }
                "definelong" => {
                    let end = block_end(body, at, "definelong", "enddefinelong")?;
                    let text: Vec<&str> = body[at + 1..end].iter().map(|l| l.text).collect();
                    self.define(rest, Some(text.join("\n")), line.no)?;
                    at = end + 1;
                    continue;
                }
                "return" => {
                    if !frame.function {
                        return Err(fail(line.no, "`!return` is only for a function"));
                    }
                    return Ok(Flow::Return(self.eval_text(rest, line.no, frame)?));
                }
                "define" => self.define(rest, None, line.no)?,
                "undef" => {
                    self.defines.remove(rest);
                }
                "global" | "local" => self.assign(rest, line.no, frame, Some(word == "global"))?,
                "assert" => self.assert(rest, line.no, frame)?,
                "include" | "include_many" | "include_once" => {
                    self.include(&word, rest, line, frame, out)?;
                }
                "theme" => self.theme(rest, line, frame, out)?,
                // Debug output, and the markers `!includesub` looks for.
                "log" | "dump_memory" | "startsub" | "endsub" => {}
                "elseif" | "else" | "endif" | "endwhile" | "endfunction" | "endprocedure"
                | "enddefinelong" => {
                    return Err(fail(line.no, format!("`!{word}` has no start")));
                }
                _ if is_assignment(directive) => self.assign(directive, line.no, frame, None)?,
                _ => {
                    let shown = if word.is_empty() {
                        directive.chars().take(20).collect()
                    } else {
                        word
                    };
                    return Err(fail(
                        line.no,
                        format!("the preprocessor's `!{shown}` is not supported yet"),
                    ));
                }
            }
            at += 1;
        }
        Ok(Flow::Next)
    }

    fn step(&mut self, no: usize) -> Result<(), Declined> {
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err(fail(
                no,
                "the preprocessor runs too long; is a `!while` endless?",
            ));
        }
        Ok(())
    }

    fn enter(&mut self, no: usize) -> Result<(), Declined> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(fail(no, "the preprocessor nests too deep"));
        }
        Ok(())
    }

    /// Runs the branch of `!if`, `!elseif` or `!else` whose test holds; the
    /// flow and the index after `!endif`.
    fn branch(
        &mut self,
        body: &[Line],
        at: usize,
        frame: &mut Frame,
        out: &mut Out,
    ) -> Result<(Flow, usize), Declined> {
        let (mut heads, end) = if_parts(body, at)?;
        heads.push(end);
        for pair in heads.windows(2) {
            let head = body[pair[0]];
            let (word, rest) = split_word(&head.text[1..]);
            let taken = match word.as_str() {
                "if" | "elseif" => self.eval_text(rest, head.no, frame)?.truthy(),
                "ifdef" => self.defined(rest, frame),
                "ifndef" => !self.defined(rest, frame),
                _ => true,
            };
            if taken {
                let flow = self.exec(&body[pair[0] + 1..pair[1]], frame, out)?;
                return Ok((flow, end + 1));
            }
        }
        Ok((Flow::Next, end + 1))
    }

    /// `!ifdef A`, also `A || B` and `A && B`.
    fn defined(&self, names: &str, frame: &Frame) -> bool {
        names.split("||").any(|any| {
            any.split("&&").all(|name| {
                let name = name.trim();
                self.defines.contains_key(name)
                    || self.callables.contains_key(name)
                    || self.lookup(name, frame).is_some()
            })
        })
    }

    fn define_callable(&mut self, body: &[Line], at: usize) -> Result<usize, Declined> {
        let line = body[at];
        let mut rest = &line.text[1..];
        let mut unquoted = false;
        let procedure = loop {
            let (word, after) = split_word(rest);
            match word.as_str() {
                "unquoted" | "final" => {
                    unquoted |= word == "unquoted";
                    rest = after.strip_prefix('!').unwrap_or(after);
                }
                "function" => break false,
                "procedure" => break true,
                _ => {
                    return Err(fail(
                        line.no,
                        format!("`!{word}` needs `!function` or `!procedure`"),
                    ));
                }
            }
        };
        let (_, rest) = split_word(rest);
        let (name, params, tail) = header(rest, line.no)?;
        let (body, next) = if let Some(expr) = tail.strip_prefix("!return") {
            if procedure {
                return Err(fail(line.no, "a procedure returns nothing"));
            }
            (Body::Return(expr.trim().to_owned(), line.no), at + 1)
        } else if tail.is_empty() {
            let close = if procedure {
                "endprocedure"
            } else {
                "endfunction"
            };
            let end = callable_end(body, at, close)?;
            let lines = body[at + 1..end].iter().map(|&line| Owned::from(line));
            (Body::Lines(lines.collect()), end + 1)
        } else {
            return Err(fail(
                line.no,
                format!("`{tail}` after the parameters is not supported"),
            ));
        };
        self.callables.insert(
            name,
            Rc::new(Callable {
                procedure,
                unquoted,
                params,
                body,
            }),
        );
        Ok(next)
    }

    /// `!define NAME`, `!define NAME text`, `!define NAME(a, b) text`, and
    /// the head of a `!definelong` with its `long` body.
    fn define(&mut self, rest: &str, long: Option<String>, no: usize) -> Result<(), Declined> {
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))
            .unwrap_or(rest.len());
        if end == 0 {
            return Err(fail(no, "`!define` needs a name"));
        }
        let name = rest[..end].to_owned();
        let after = &rest[end..];
        let (params, text) = if after.starts_with('(') {
            let close =
                closing_paren(after, 0).ok_or_else(|| fail(no, format!("`{name}(` has no `)`")))?;
            (
                Some(params(&after[1..close], no)?),
                after[close + 1..].trim(),
            )
        } else {
            (None, after.trim())
        };
        let body = match long {
            Some(lines) if text.is_empty() => lines,
            Some(_) => return Err(fail(no, "`!definelong` takes its text on the next lines")),
            None => text.to_owned(),
        };
        self.defines.insert(name, Rc::new(Define { params, body }));
        Ok(())
    }

    /// `$x = expr` and `$x ?= expr`; `global` forces the scope.
    fn assign(
        &mut self,
        text: &str,
        no: usize,
        frame: &mut Frame,
        global: Option<bool>,
    ) -> Result<(), Declined> {
        let end = text
            .char_indices()
            .find(|&(at, c)| !(c.is_alphanumeric() || c == '_' || (at == 0 && c == '$')))
            .map_or(text.len(), |(at, _)| at);
        let name = &text[..end];
        let after = text[end..].trim_start();
        let (only_new, expr) = if let Some(expr) = after.strip_prefix("?=") {
            (true, expr)
        } else if let Some(expr) = after.strip_prefix('=')
            && !expr.starts_with('=')
        {
            (false, expr)
        } else {
            return Err(fail(no, format!("`!{name}` needs `=` and a value")));
        };
        if name.is_empty() || name == "$" {
            return Err(fail(no, "a variable needs a name"));
        }
        if only_new && self.lookup(name, frame).is_some() {
            return Ok(());
        }
        let value = self.eval_text(expr, no, frame)?;
        self.store(name, value, frame, global);
        Ok(())
    }

    /// Inside a function or procedure a variable is local, unless a global
    /// one of that name exists or `!global` says so.
    fn store(&mut self, name: &str, value: Value, frame: &mut Frame, global: Option<bool>) {
        match (&mut frame.locals, global) {
            (Some(locals), Some(false)) => {
                locals.insert(name.to_owned(), value);
            }
            (Some(locals), None)
                if locals.contains_key(name) || !self.globals.contains_key(name) =>
            {
                locals.insert(name.to_owned(), value);
            }
            _ => {
                self.globals.insert(name.to_owned(), value);
            }
        }
    }

    fn lookup(&self, name: &str, frame: &Frame) -> Option<Value> {
        frame
            .locals
            .as_ref()
            .and_then(|locals| locals.get(name))
            .or_else(|| self.globals.get(name))
            .cloned()
    }

    /// `!assert expr` or `!assert expr : "message"`.
    fn assert(&mut self, rest: &str, no: usize, frame: &mut Frame) -> Result<(), Declined> {
        let (test, message) = match split_outside(rest, ':') {
            Some((test, message)) => (test, Some(message.trim().trim_matches('"'))),
            None => (rest, None),
        };
        if self.eval_text(test, no, frame)?.truthy() {
            return Ok(());
        }
        Err(fail(
            no,
            match message {
                Some(message) => format!("`!assert` fails: {message}"),
                None => format!("`!assert {}` fails", test.trim()),
            },
        ))
    }

    /// `!theme plain`. Other names decline: Omaspec does not ship them,
    /// and PlantUML itself draws an error page for a theme it does not have.
    ///
    /// The lines are the part of `puml-theme-plain` the behavior diagrams
    /// draw. `$BGCOLOR`, `$FGCOLOR` and `$FONT_NAME` set before the theme
    /// are kept, as the theme checks them. Left out, because a diagram
    /// declines when a setting names something the engine does not draw:
    /// margins, padding, hyperlinks, monospaced and per-element font names,
    /// `LifelineStrategy`, `UseBetaStyle`, and the class, icon and stereotype
    /// skinparams.
    fn theme(
        &mut self,
        rest: &str,
        line: Line,
        frame: &mut Frame,
        out: &mut Out,
    ) -> Result<(), Declined> {
        if !rest.eq_ignore_ascii_case("plain") {
            return Err(fail(
                line.no,
                "the preprocessor's `!theme` is not supported yet",
            ));
        }
        let lines = plain_theme(line.no);
        self.enter(line.no)?;
        let flow = self.exec(&lines, frame, out)?;
        self.depth -= 1;
        if let Flow::Return(_) = flow {
            return Err(fail(line.no, "`!return` outside a function"));
        }
        Ok(())
    }

    /// `!include`, `!include_many` and `!include_once`: runs the file's lines
    /// here. They carry the line of the `!include`, and what declines in them
    /// names the file. As in PlantUML, `!include` takes a file once and
    /// `!include_once` declines a second time.
    fn include(
        &mut self,
        word: &str,
        rest: &str,
        line: Line,
        frame: &mut Frame,
        out: &mut Out,
    ) -> Result<(), Declined> {
        let Some(includes) = self.includes else {
            return Err(fail(
                line.no,
                format!("`!{word}` needs the diagram's folder; save the diagram first"),
            ));
        };
        let expanded = self.expand(rest, line.no, frame, 0)?;
        let target = unquote(&expanded);
        if target.is_empty() {
            return Err(fail(line.no, format!("`!{word}` needs a file")));
        }
        if let Some((_, part)) = target.rsplit_once('!')
            && !part.contains(['/', '\\'])
        {
            return Err(fail(
                line.no,
                format!("including one diagram of a file (`!{part}`) is not supported yet"),
            ));
        }
        let from = self.files.last().map(String::as_str);
        let file = includes
            .read(target, from)
            .map_err(|reason| fail(line.no, reason))?;
        let again = !self.included.insert(file.key.clone());
        match word {
            "include" if again => return Ok(()),
            "include_once" if again => {
                return Err(fail(line.no, format!("`{target}` is already included")));
            }
            _ => {}
        }
        self.included_bytes += file.text.len();
        if self.included.len() > MAX_INCLUDES || self.included_bytes > MAX_INCLUDED {
            return Err(fail(line.no, "the diagram includes too much"));
        }
        let top = frame.call.unwrap_or(line.no);
        let in_file = |err: Declined| Declined {
            line: Some(top),
            reason: format!("in `{target}`: {}", err.reason),
        };
        let (_, lines) = source::body(&file.text).map_err(in_file)?;
        let lines: Vec<Line> = lines.into_iter().map(|l| Line { no: top, ..l }).collect();
        self.enter(line.no)?;
        self.files.push(file.key);
        let flow = self.exec(&lines, frame, out).map_err(in_file)?;
        self.files.pop();
        self.depth -= 1;
        if let Flow::Return(_) = flow {
            return Err(fail(
                top,
                format!("in `{target}`: `!return` outside a function"),
            ));
        }
        Ok(())
    }

    fn text_line(&mut self, line: Line, frame: &mut Frame, out: &mut Out) -> Result<(), Declined> {
        if frame.function {
            return Err(fail(
                line.no,
                "a function writes no lines; a procedure does",
            ));
        }
        if let Some((name, raw)) = whole_call(line.text)
            && let Some(callable) = self.callables.get(name).filter(|c| c.procedure).cloned()
        {
            out.blanks += line.blank_before;
            let args = args_of(raw, callable.unquoted, line.no)?;
            let locals = self.bind(&callable, name, &args, line.no, frame)?;
            let mut inner = Frame {
                locals: Some(locals),
                function: false,
                call: Some(frame.call.unwrap_or(line.no)),
            };
            self.enter(line.no)?;
            if let Body::Lines(lines) = &callable.body {
                let lines: Vec<Line> = lines.iter().map(Owned::line).collect();
                self.exec(&lines, &mut inner, out)?;
            }
            self.depth -= 1;
            return Ok(());
        }
        let text = self.expand(line.text, line.no, frame, 0)?;
        out.write(line, frame.call, &text)
    }

    /// `text` with its variables, `!define`s and calls replaced.
    fn expand(
        &mut self,
        text: &str,
        no: usize,
        frame: &mut Frame,
        depth: usize,
    ) -> Result<String, Declined> {
        let mut out = String::with_capacity(text.len());
        let mut again = false;
        let mut at = 0;
        while let Some(c) = text[at..].chars().next() {
            let after = &text[at + c.len_utf8()..];
            let starts = match c {
                '$' | '%' => after.starts_with(|n: char| n.is_alphabetic() || n == '_'),
                _ => {
                    (c.is_alphabetic() || c == '_')
                        && !text[..at].ends_with(|p: char| p.is_alphanumeric() || p == '_')
                }
            };
            if !starts {
                out.push(c);
                at += c.len_utf8();
                continue;
            }
            let end = at
                + c.len_utf8()
                + after
                    .find(|n: char| !(n.is_alphanumeric() || n == '_'))
                    .unwrap_or(after.len());
            let name = &text[at..end];
            if text[end..].starts_with('(') && self.calls(name) {
                let close = closing_paren(text, end)
                    .ok_or_else(|| fail(no, format!("`{name}(` has no `)`")))?;
                let value =
                    self.call_in_text(name, &text[end + 1..close], no, frame, &mut again)?;
                out.push_str(&value);
                at = close + 1;
            } else {
                if let Some(value) = self.lookup(name, frame) {
                    out.push_str(&value.to_string());
                } else if let Some(define) = self.defines.get(name).filter(|d| d.params.is_none()) {
                    out.push_str(&define.body);
                    again = true;
                } else {
                    out.push_str(name);
                }
                at = end;
            }
            if out.len() > MAX_TEXT {
                return Err(fail(no, "a line grows too long in the preprocessor"));
            }
        }
        if again {
            if depth >= MAX_DEPTH {
                return Err(fail(no, "the `!define`s expand without end"));
            }
            return self.expand(&out, no, frame, depth + 1);
        }
        Ok(out)
    }

    /// Whether `name(` in a line is a call the preprocessor runs.
    fn calls(&self, name: &str) -> bool {
        name.starts_with('%')
            || self.callables.contains_key(name)
            || self.defines.get(name).is_some_and(|d| d.params.is_some())
    }

    fn call_in_text(
        &mut self,
        name: &str,
        raw: &str,
        no: usize,
        frame: &mut Frame,
        again: &mut bool,
    ) -> Result<String, Declined> {
        if let Some(callable) = self.callables.get(name).cloned() {
            if callable.procedure {
                return Err(fail(
                    no,
                    format!("the procedure `{name}` has to be alone on its line"),
                ));
            }
            let args = args_of(raw, callable.unquoted, no)?;
            return Ok(self.call(name, &args, no, frame)?.to_string());
        }
        if let Some(define) = self.defines.get(name).cloned() {
            *again = true;
            return macro_text(name, &define, raw, no);
        }
        let args = args_of(raw, false, no)?;
        Ok(self.call(name, &args, no, frame)?.to_string())
    }

    fn eval_text(&mut self, text: &str, no: usize, frame: &mut Frame) -> Result<Value, Declined> {
        let expr = expr::parse(text).map_err(|reason| fail(no, reason))?;
        self.eval(&expr, no, frame)
    }

    fn eval(&mut self, expr: &Expr, no: usize, frame: &mut Frame) -> Result<Value, Declined> {
        self.evals += 1;
        let value = if self.evals > MAX_EVAL {
            Err(fail(no, "an expression nests too deep in the preprocessor"))
        } else {
            self.eval_node(expr, no, frame)
        };
        self.evals -= 1;
        value
    }

    fn eval_node(&mut self, expr: &Expr, no: usize, frame: &mut Frame) -> Result<Value, Declined> {
        Ok(match expr {
            Expr::Lit(value) => value.clone(),
            Expr::Var(name) => self
                .lookup(name, frame)
                .or_else(|| {
                    self.defines
                        .get(name)
                        .filter(|d| d.params.is_none())
                        .map(|d| Value::Str(d.body.clone()))
                })
                .ok_or_else(|| fail(no, format!("`{name}` is not defined")))?,
            Expr::Call(name, args) => self.call(name, args, no, frame)?,
            Expr::Neg(inner) => {
                let value = self.eval(inner, no, frame)?;
                let n = value
                    .number()
                    .and_then(i64::checked_neg)
                    .ok_or_else(|| fail(no, format!("`-` needs a number, not `{value}`")))?;
                Value::Int(n)
            }
            Expr::Not(inner) => Value::from_bool(!self.eval(inner, no, frame)?.truthy()),
            Expr::Bin(..) => self.chain(expr, no, frame)?,
        })
    }

    /// Operators from left to right, without going down the left side of
    /// the tree: `1 + 1 + …` is as deep as it is long.
    fn chain(&mut self, expr: &Expr, no: usize, frame: &mut Frame) -> Result<Value, Declined> {
        let mut rights = Vec::new();
        let mut first = expr;
        while let Expr::Bin(op, a, b) = first {
            rights.push((*op, b));
            first = a;
        }
        let mut value = self.eval(first, no, frame)?;
        for (op, b) in rights.into_iter().rev() {
            value = match op {
                Op::And => Value::from_bool(value.truthy() && self.eval(b, no, frame)?.truthy()),
                Op::Or => Value::from_bool(value.truthy() || self.eval(b, no, frame)?.truthy()),
                _ => {
                    let b = self.eval(b, no, frame)?;
                    let value = expr::binary(op, value, b).map_err(|reason| fail(no, reason))?;
                    if matches!(&value, Value::Str(s) if s.len() > MAX_TEXT) {
                        return Err(fail(no, "a value grows too long in the preprocessor"));
                    }
                    value
                }
            };
        }
        Ok(value)
    }

    /// Calls a function or a built-in and returns its value.
    fn call(
        &mut self,
        name: &str,
        args: &[Arg],
        no: usize,
        frame: &mut Frame,
    ) -> Result<Value, Declined> {
        if name.starts_with('%') {
            return self.builtin(name, args, no, frame);
        }
        let Some(callable) = self.callables.get(name).cloned() else {
            return Err(fail(no, format!("there is no function `{name}`")));
        };
        if callable.procedure {
            return Err(fail(no, format!("the procedure `{name}` returns nothing")));
        }
        let locals = self.bind(&callable, name, args, no, frame)?;
        let mut inner = Frame {
            locals: Some(locals),
            function: true,
            call: frame.call,
        };
        self.enter(no)?;
        let value = match &callable.body {
            Body::Return(text, line) => self.eval_text(text, *line, &mut inner)?,
            Body::Lines(lines) => {
                let lines: Vec<Line> = lines.iter().map(Owned::line).collect();
                match self.exec(&lines, &mut inner, &mut Out::default())? {
                    Flow::Return(value) => value,
                    Flow::Next => Value::Str(String::new()),
                }
            }
        };
        self.depth -= 1;
        if value.to_string().len() > MAX_TEXT {
            return Err(fail(no, format!("`{name}` returns too much text")));
        }
        Ok(value)
    }

    /// The parameters of a call, by position or by name, defaults filled in.
    fn bind(
        &mut self,
        callable: &Callable,
        name: &str,
        args: &[Arg],
        no: usize,
        frame: &mut Frame,
    ) -> Result<HashMap<String, Value>, Declined> {
        let params = &callable.params;
        let mut values: Vec<Option<Value>> = vec![None; params.len()];
        let mut next = 0;
        for arg in args {
            let slot = match &arg.name {
                Some(given) => params
                    .iter()
                    .position(|p| p.name == *given)
                    .ok_or_else(|| fail(no, format!("`{name}` has no parameter `{given}`")))?,
                None => {
                    next += 1;
                    next - 1
                }
            };
            if slot >= params.len() {
                return Err(fail(
                    no,
                    format!("`{name}` takes {} argument(s)", params.len()),
                ));
            }
            values[slot] = Some(self.eval(&arg.expr, no, frame)?);
        }
        let mut locals = HashMap::new();
        for (param, value) in params.iter().zip(values) {
            let value = match (value, &param.default) {
                (Some(value), _) => value,
                (None, Some(default)) => self.eval_text(default, no, frame)?,
                (None, None) => {
                    return Err(fail(no, format!("`{name}` needs `{}`", param.name)));
                }
            };
            locals.insert(param.name.clone(), value);
        }
        Ok(locals)
    }

    fn builtin(
        &mut self,
        name: &str,
        args: &[Arg],
        no: usize,
        frame: &mut Frame,
    ) -> Result<Value, Declined> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            if arg.name.is_some() {
                return Err(fail(no, format!("`{name}` takes no named arguments")));
            }
            values.push(self.eval(&arg.expr, no, frame)?);
        }
        let first = || values.first().map(Value::to_string).unwrap_or_default();
        Ok(match name {
            "%variable_exists" => Value::from_bool(self.lookup(&first(), frame).is_some()),
            "%function_exists" => {
                let wanted = first();
                Value::from_bool(self.callables.contains_key(&wanted) || builtin::knows(&wanted))
            }
            "%get_variable_value" => self
                .lookup(&first(), frame)
                .unwrap_or(Value::Str(String::new())),
            "%set_variable_value" => {
                let Some(value) = values.get(1).cloned() else {
                    return Err(fail(no, "`%set_variable_value` needs a name and a value"));
                };
                self.store(&first(), value, frame, None);
                Value::Str(String::new())
            }
            "%call_user_func" => {
                let rest: Vec<Arg> = values
                    .iter()
                    .skip(1)
                    .map(|value| Arg {
                        name: None,
                        expr: Expr::Lit(value.clone()),
                    })
                    .collect();
                self.call(&first(), &rest, no, frame)?
            }
            _ => builtin::call(name, &values).map_err(|reason| fail(no, reason))?,
        })
    }
}

/// The directive's word in lowercase, and what follows it, trimmed.
fn split_word(text: &str) -> (String, &str) {
    let end = text
        .find(|c: char| !(c.is_ascii_alphabetic() || c == '_'))
        .unwrap_or(text.len());
    (text[..end].to_ascii_lowercase(), text[end..].trim())
}

/// `$x = …`, `x ?= …`: a name, then `=` or `?=`.
fn is_assignment(directive: &str) -> bool {
    let rest = directive.strip_prefix('$').unwrap_or(directive);
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    let after = rest[end..].trim_start();
    end > 0 && (after.starts_with("?=") || (after.starts_with('=') && !after.starts_with("==")))
}

/// The heads of `!if`, `!elseif` and `!else` that start at `at`, and the
/// index of its `!endif`.
fn if_parts(body: &[Line], at: usize) -> Result<(Vec<usize>, usize), Declined> {
    let mut heads = vec![at];
    let mut depth = 0usize;
    let mut after_else = false;
    for (index, line) in body.iter().enumerate().skip(at + 1) {
        let Some(directive) = line.text.strip_prefix('!') else {
            continue;
        };
        match split_word(directive).0.as_str() {
            "if" | "ifdef" | "ifndef" => depth += 1,
            "endif" if depth == 0 => return Ok((heads, index)),
            "endif" => depth -= 1,
            word @ ("elseif" | "else") if depth == 0 => {
                if after_else {
                    return Err(fail(line.no, format!("`!{word}` after `!else`")));
                }
                after_else = word == "else";
                heads.push(index);
            }
            _ => {}
        }
    }
    Err(fail(body[at].no, "`!if` has no `!endif`"))
}

/// The index of the `!close` that ends the block `!open` starts at `at`.
fn block_end(body: &[Line], at: usize, open: &str, close: &str) -> Result<usize, Declined> {
    let mut depth = 0usize;
    for (index, line) in body.iter().enumerate().skip(at + 1) {
        let Some(directive) = line.text.strip_prefix('!') else {
            continue;
        };
        let word = split_word(directive).0;
        if word == open {
            depth += 1;
        } else if word == close {
            if depth == 0 {
                return Ok(index);
            }
            depth -= 1;
        }
    }
    Err(fail(body[at].no, format!("`!{open}` has no `!{close}`")))
}

/// The `!endfunction` or `!endprocedure` of the definition at `at`; another
/// definition inside it declines.
fn callable_end(body: &[Line], at: usize, close: &str) -> Result<usize, Declined> {
    for (index, line) in body.iter().enumerate().skip(at + 1) {
        let Some(directive) = line.text.strip_prefix('!') else {
            continue;
        };
        match split_word(directive).0.as_str() {
            word if word == close => return Ok(index),
            "function" | "procedure" | "unquoted" | "final" => {
                return Err(fail(
                    line.no,
                    "a definition inside a definition is not supported",
                ));
            }
            _ => {}
        }
    }
    Err(fail(
        body[at].no,
        format!("the definition has no `!{close}`"),
    ))
}

/// `$name($a, $b = "x")` and what follows it.
fn header(text: &str, no: usize) -> Result<(String, Vec<Param>, &str), Declined> {
    let open = text
        .find('(')
        .ok_or_else(|| fail(no, "a function needs `(` and its parameters"))?;
    let name = text[..open].trim();
    let valid = name
        .strip_prefix('$')
        .unwrap_or(name)
        .chars()
        .enumerate()
        .all(|(i, c)| c.is_alphabetic() || c == '_' || (i > 0 && c.is_alphanumeric()));
    if name.is_empty() || name == "$" || !valid {
        return Err(fail(no, format!("`{name}` is no function name")));
    }
    let close = closing_paren(text, open).ok_or_else(|| fail(no, "a `(` is not closed"))?;
    Ok((
        name.to_owned(),
        params(&text[open + 1..close], no)?,
        text[close + 1..].trim(),
    ))
}

fn params(text: &str, no: usize) -> Result<Vec<Param>, Declined> {
    split_args(text)
        .into_iter()
        .map(|param| {
            let (name, default) = match split_outside(param, '=') {
                Some((name, default)) => (name.trim(), Some(default.trim().to_owned())),
                None => (param, None),
            };
            if name.is_empty() || name.contains(char::is_whitespace) {
                return Err(fail(no, format!("`{param}` is no parameter")));
            }
            // As in PlantUML: `$x`, `x`, `$x_1`.
            let bare = name.strip_prefix('$').unwrap_or(name);
            if !bare.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return Err(fail(no, format!("`{name}` is no parameter name")));
            }
            Ok(Param {
                name: name.to_owned(),
                default,
            })
        })
        .collect()
}

/// The arguments written in a line: text for `!unquoted`, else expressions.
fn args_of(raw: &str, unquoted: bool, no: usize) -> Result<Vec<Arg>, Declined> {
    split_args(raw)
        .into_iter()
        .map(|arg| {
            if unquoted {
                return Ok(Arg {
                    name: None,
                    expr: Expr::Lit(Value::Str(unquote(arg).to_owned())),
                });
            }
            expr::parse_arg(arg).map_err(|reason| fail(no, reason))
        })
        .collect()
}

/// The text of a `!define` with parameters, its arguments put in.
fn macro_text(name: &str, define: &Define, raw: &str, no: usize) -> Result<String, Declined> {
    let params = define.params.as_deref().unwrap_or_default();
    let args = split_args(raw);
    if args.len() > params.len() {
        return Err(fail(
            no,
            format!("`{name}` takes {} argument(s)", params.len()),
        ));
    }
    let mut values = HashMap::new();
    for (index, param) in params.iter().enumerate() {
        let value = match (args.get(index), &param.default) {
            (Some(arg), _) => (*arg).to_owned(),
            (None, Some(default)) => unquote(default).to_owned(),
            (None, None) => return Err(fail(no, format!("`{name}` needs `{}`", param.name))),
        };
        values.insert(param.name.as_str(), value);
    }
    let mut out = String::with_capacity(define.body.len());
    let body = &define.body;
    let mut at = 0;
    while let Some(c) = body[at..].chars().next() {
        let starts = (c.is_alphabetic() || c == '_' || c == '$')
            && !body[..at].ends_with(|p: char| p.is_alphanumeric() || p == '_');
        if !starts {
            out.push(c);
            at += c.len_utf8();
            continue;
        }
        let end = at
            + c.len_utf8()
            + body[at + c.len_utf8()..]
                .find(|n: char| !(n.is_alphanumeric() || n == '_'))
                .unwrap_or(body.len() - at - c.len_utf8());
        let word = &body[at..end];
        out.push_str(values.get(word).map_or(word, String::as_str));
        at = end;
    }
    Ok(out.replace("##", ""))
}

/// `name(args)` that is the whole line: the name and the raw arguments.
fn whole_call(text: &str) -> Option<(&str, &str)> {
    let open = text.find('(')?;
    let name = &text[..open];
    let bare = name.strip_prefix('$').unwrap_or(name);
    if bare.is_empty() || !bare.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let close = closing_paren(text, open)?;
    (close == text.len() - 1).then(|| (name, &text[open + 1..close]))
}

/// The index of the `)` that closes the `(` at `open`. Quotes hide
/// parentheses; a `'` counts as a quote only where an argument starts.
fn closing_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut last = '(';
    for (index, c) in text[open..].char_indices() {
        let index = open + index;
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' => quote = Some('"'),
            '\'' if matches!(last, '(' | ',') => quote = Some('\''),
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        if !c.is_whitespace() {
            last = c;
        }
    }
    None
}

/// The arguments in `text`, split at the commas outside quotes and
/// parentheses, trimmed. No text is no argument.
fn split_args(text: &str) -> Vec<&str> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (index, c) in text.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' => quote = Some('"'),
            '\'' if text[start..index].trim().is_empty() => quote = Some('\''),
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(text[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    out.push(text[start..].trim());
    out
}

/// `text` split at the first `sep` outside quotes.
fn split_outside(text: &str, sep: char) -> Option<(&str, &str)> {
    let mut quote: Option<char> = None;
    for (index, c) in text.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == sep => return Some((&text[..index], &text[index + 1..])),
            None => {}
        }
    }
    None
}

/// `"text"` or `'text'` without its quotes.
fn unquote(text: &str) -> &str {
    let text = text.trim();
    for q in ['"', '\''] {
        if let Some(inner) = text.strip_prefix(q).and_then(|t| t.strip_suffix(q)) {
            return inner;
        }
    }
    text
}

fn fail(line: usize, reason: impl Into<String>) -> Declined {
    Declined::at(line, reason)
}

/// The plain theme's lines, each carrying the `!theme` line so a decline
/// there names the call.
fn plain_theme(no: usize) -> Vec<Line<'static>> {
    PLAIN_THEME
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|text| Line {
            no,
            text,
            indent: 0,
            blank_before: 0,
        })
        .collect()
}

/// What Omaspec draws of PlantUML's `plain` theme. The variables are the
/// theme's own, so a diagram can set them first.
const PLAIN_THEME: &str = r#"
!$THEME = "plain"
!if %not(%variable_exists("$BGCOLOR"))
!$BGCOLOR = "white"
!endif
!if %not(%variable_exists("$FGCOLOR"))
!$FGCOLOR = "black"
!endif
!ifndef $FONT_NAME
!$FONT_NAME = "Verdana"
!endif
<style>
root {
BackgroundColor $BGCOLOR
FontColor $FGCOLOR
FontName $FONT_NAME
LineColor $FGCOLOR
LineThickness 1
}
document {
title {
FontSize 22
}
}
</style>
skinparam BackgroundColor $BGCOLOR
skinparam DefaultFontName $FONT_NAME
skinparam Shadowing false
skinparam ArrowColor $FGCOLOR
skinparam SequenceLifeLineBorderColor $FGCOLOR
skinparam State {
BackgroundColor $BGCOLOR
BorderColor $FGCOLOR
}
skinparam Activity {
BackgroundColor $BGCOLOR
BarColor $FGCOLOR
BorderColor $FGCOLOR
FontColor $FGCOLOR
}
skinparam Note {
FontColor $FGCOLOR
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::body;

    /// Files by path; a path in a folder resolves against the including
    /// file's folder, as a real one would.
    struct Files(Vec<(&'static str, &'static str)>);

    impl Includes for Files {
        fn read(&self, path: &str, from: Option<&str>) -> Result<Included, String> {
            let dir = from.and_then(|f| f.rsplit_once('/')).map(|(dir, _)| dir);
            let key = match dir {
                Some(dir) => format!("{dir}/{path}"),
                None => path.to_owned(),
            };
            self.0
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, text)| Included {
                    key: key.clone(),
                    text: (*text).to_owned(),
                })
                .ok_or_else(|| format!("`{path}` is not there"))
        }
    }

    fn with_files(source: &str, files: Files) -> Result<Vec<(usize, String)>, Declined> {
        let lines = body(source).unwrap().1;
        Ok(run(&lines, Some(&files))?
            .expect("the preprocessor ran")
            .into_iter()
            .map(|line| (line.no, line.text))
            .collect())
    }

    #[test]
    fn the_same_lines_give_what_they_gave_before() {
        let lines = |source: &str| -> Result<Vec<(usize, String)>, Declined> {
            let lines = body(source).unwrap().1;
            Ok(run(&lines, None)?
                .expect("the preprocessor ran")
                .into_iter()
                .map(|line| (line.no, line.text))
                .collect())
        };
        let a = "!$x = 1\nA -> B : $x";
        assert_eq!(lines(a).unwrap(), [(1, "A -> B : 1".into())]);
        assert_eq!(lines(a).unwrap(), [(1, "A -> B : 1".into())]);
        // Another text, or the same text a line further down, runs again.
        assert_eq!(
            lines("!$x = 2\nA -> B : $x").unwrap(),
            [(1, "A -> B : 2".into())]
        );
        assert_eq!(
            lines(&format!("\n{a}")).unwrap(),
            [(2, "A -> B : 1".into())]
        );
        // A loop that never ends declines each time, the second at once.
        let endless = "!$i = 1\n!while $i > 0\nA -> B\n!endwhile";
        let first = lines(endless).unwrap_err();
        let started = std::time::Instant::now();
        assert_eq!(lines(endless).unwrap_err(), first);
        assert!(started.elapsed() < std::time::Duration::from_millis(50));
        // With files to include, nothing is kept: a file may change.
        let files = || Files(vec![("x.puml", "!$x = 3")]);
        let source = "!include x.puml\nA -> B : $x";
        assert_eq!(
            with_files(source, files()).unwrap(),
            [(1, "A -> B : 3".into())]
        );
        let changed = Files(vec![("x.puml", "!$x = 4")]);
        assert_eq!(
            with_files(source, changed).unwrap(),
            [(1, "A -> B : 4".into())]
        );
    }

    #[test]
    fn includes_run_in_place_and_carry_the_include_line() {
        let files = || {
            Files(vec![
                (
                    "common.puml",
                    "@startuml\n' shared\n!$server = \"Api\"\n!procedure $hello($to)\nUser -> $to : hello\n!endprocedure\n!include parts/more.puml\n@enduml",
                ),
                ("parts/more.puml", "participant $server"),
                ("bad.puml", "!if 1"),
            ])
        };
        let source = "A -> B\n!include common.puml\n!include common.puml\n$hello($server)";
        assert_eq!(
            with_files(source, files()).unwrap(),
            [
                (0, "A -> B".into()),
                (1, "participant Api".into()),
                (3, "User -> Api : hello".into()),
            ]
        );
        // `!include_many` takes it again, `!include_once` declines.
        let many = with_files(
            "!include common.puml\n!include_many parts/more.puml",
            files(),
        );
        assert_eq!(many.unwrap().len(), 2);
        let once = "!include_once parts/more.puml\n!include_once parts/more.puml";
        let err = with_files(once, files()).unwrap_err();
        assert_eq!(err.line, Some(1));
        assert!(err.reason.contains("already included"), "{}", err.reason);
        // What declines inside names the file, on the `!include` line.
        let err = with_files("A -> B\n!include bad.puml", files()).unwrap_err();
        assert_eq!(err.line, Some(1));
        assert!(err.reason.contains("in `bad.puml`"), "{}", err.reason);
        let err = with_files("!include nope.puml", files()).unwrap_err();
        assert!(err.reason.contains("not there"), "{}", err.reason);
        let err = with_files("!include common.puml!2", files()).unwrap_err();
        assert!(
            err.reason.contains("one diagram of a file"),
            "{}",
            err.reason
        );
        // Without a folder, `!include` cannot read anything.
        let err = declined("!include common.puml");
        assert!(
            err.reason.contains("save the diagram first"),
            "{}",
            err.reason
        );
    }

    #[test]
    fn deep_evaluation_ends_and_long_chains_run() {
        let function = |body: &str| {
            format!(
                "!function $f($n)\n!if $n <= 0\n!return 0\n!endif\n!return {body}\n!endfunction\nA -> B : $f(60)"
            )
        };
        // A chain runs in a loop, so sixty calls in chains of 990 run.
        let chain = function(&format!("$f($n - 1){}", " + 1".repeat(990)));
        assert_eq!(texts(&chain), ["A -> B : 59400"]);
        // Signs around the call add up across the calls, and end.
        let signs = function(&format!("{}$f($n - 1)", "-".repeat(98)));
        let err = declined(&signs);
        assert!(err.reason.contains("nests too deep"), "{}", err.reason);
    }

    #[test]
    fn including_itself_ends() {
        let files = Files(vec![("loop.puml", "!include_many loop.puml")]);
        let err = with_files("!include_many loop.puml", files).unwrap_err();
        assert!(err.reason.contains("nests too deep"), "{}", err.reason);
    }

    /// The lines after preprocessing, with their line numbers.
    fn run_on(source: &str) -> Vec<(usize, String)> {
        let lines = body(source).unwrap().1;
        run(&lines, None)
            .unwrap_or_else(|err| panic!("{err:?}"))
            .expect("the preprocessor ran")
            .into_iter()
            .map(|line| (line.no, line.text))
            .collect()
    }

    fn texts(source: &str) -> Vec<String> {
        run_on(source).into_iter().map(|(_, text)| text).collect()
    }

    fn declined(source: &str) -> Declined {
        let lines = body(source).unwrap().1;
        run(&lines, None).expect_err("should decline")
    }

    #[test]
    fn leaves_plain_diagrams_alone() {
        let lines = body("A -> B : 50% $5\nnote over A : (x)").unwrap().1;
        assert_eq!(run(&lines, None), Ok(None));
    }

    #[test]
    fn variables_and_conditions() {
        let source = "!$who = \"Alice\"\n!$n = 2\n!$n ?= 5\n!if $n > 1 && $who == 'Alice'\n$who -> Bob : $n times\n!elseif %true()\nnever\n!else\nnever\n!endif\n!ifndef $missing\nBob -> $who\n!endif";
        assert_eq!(texts(source), ["Alice -> Bob : 2 times", "Bob -> Alice"]);
    }

    #[test]
    fn old_defines_and_macros() {
        let source = "!define SERVER Web\n!define CALL(a, b=\"ok\") a -> SERVER : b\n!define JOIN(x) x##_id\nCALL(User)\nCALL(Admin, done)\nJOIN(user)\n!undef SERVER\nSERVER\n!ifdef CALL\nyes\n!endif";
        assert_eq!(
            texts(source),
            [
                "User -> Web : ok",
                "Admin -> Web : done",
                "user_id",
                "SERVER",
                "yes"
            ]
        );
    }

    #[test]
    fn procedures_write_lines_on_the_call_line() {
        let source = "!procedure $msg($from, $to = \"Bob\")\n  $from -> $to : hi\n  $to --> $from\n!endprocedure\n\n$msg(\"Alice\")\n$msg(\"Carol\", $to = \"Dave\")";
        assert_eq!(
            run_on(source),
            [
                (5, "Alice -> Bob : hi".into()),
                (5, "Bob --> Alice".into()),
                (6, "Carol -> Dave : hi".into()),
                (6, "Dave --> Carol".into()),
            ]
        );
    }

    #[test]
    fn functions_loops_and_built_ins() {
        let source = "!function $twice($x) !return $x * 2\n!function $label($name)\n!$upper = %upper($name)\n!return $upper + \" (\" + %strlen($name) + \")\"\n!endfunction\n!unquoted function $raw($text) !return $text\n!$i = 0\n!while $i < 3\nstep $i is $twice($i)\n!$i = $i + 1\n!endwhile\ntitle $label(\"shop\")\nA -> B : $raw(not quoted)\nline%newline()next";
        assert_eq!(
            texts(source),
            [
                "step 0 is 0",
                "step 1 is 2",
                "step 2 is 4",
                "title SHOP (4)",
                "A -> B : not quoted",
                "line",
                "next"
            ]
        );
    }

    #[test]
    fn variables_in_procedures_are_local_unless_global() {
        let source = "!$count = 0\n!procedure $add()\n!$count = $count + 1\n!$seen = 1\n!endprocedure\n$add()\n$add()\ncount $count\n!if %variable_exists(\"$seen\")\nleaked\n!endif";
        assert_eq!(texts(source), ["count 2"]);
    }

    #[test]
    fn plain_theme_keeps_colors_set_before_it() {
        let lines = texts("!$BGCOLOR = \"#123456\"\n!theme plain\nA -> B");
        assert!(
            lines
                .iter()
                .any(|l| l == "skinparam BackgroundColor #123456"),
            "{lines:?}"
        );
        assert!(lines.iter().any(|l| l == "FontSize 22"), "{lines:?}");
        assert!(lines.iter().any(|l| l == "A -> B"), "{lines:?}");
        assert!(!lines.iter().any(|l| l.contains("$BGCOLOR")), "{lines:?}");

        let drawn = |source: &str| {
            crate::parse(source).and_then(|d| match d {
                crate::Diagram::State(d) => crate::state::draw(&d),
                crate::Diagram::Sequence(d) => crate::sequence::draw(&d),
                crate::Diagram::Activity(d) => crate::activity::draw(&d),
                other => panic!("unexpected {other:?}"),
            })
        };
        let state = drawn("@startuml\n!theme plain\ntitle Hello\n[*] --> Idle\n@enduml").unwrap();
        assert!(state.contains("font-size=\"22\""), "{state}");
        assert!(!state.contains("#FEFFDD"), "{state}");
        assert!(state.contains("#FFFFFF"), "{state}");
        let tinted =
            drawn("@startuml\n!$BGCOLOR = \"#123456\"\n!theme plain\n[*] --> Idle\n@enduml")
                .unwrap();
        assert!(tinted.contains("#123456"), "{tinted}");
        drawn("@startuml\n!theme plain\nA -> B : hi\n@enduml").unwrap();
        drawn("@startuml\n!theme plain\nstart\n:go;\nstop\n@enduml").unwrap();

        let table = drawn(
            "@startuml\n[*] --> Idle\nnote right of Idle\n|= a |= b |\n| 1 | 2 |\nend note\nlegend\n|= a |= b |\n| 1 | 2 |\nendlegend\n@enduml",
        )
        .unwrap();
        for word in ["a", "b", "1", "2"] {
            assert!(
                table.contains(&format!(">{word}<")),
                "{word} missing in {table}"
            );
        }
        assert!(table.contains("<line "), "{table}");
    }

    #[test]
    fn declines_what_it_cannot_do_and_names_the_line() {
        for (source, line, part) in [
            ("A -> B\n!include common.puml", 1, "!include"),
            ("!includeurl https://example.com/x.puml", 0, "!includeurl"),
            ("!import lib.zip", 0, "!import"),
            ("!theme cerulean", 0, "!theme"),
            ("!pragma teoz true", 0, "!pragma"),
            ("!foreach $x in [1]", 0, "!foreach"),
            ("title %date()", 0, "%date"),
            ("!if $nope\n!endif", 0, "`$nope` is not defined"),
            ("!if 1\nA -> B", 0, "`!endif`"),
            ("!else", 0, "has no start"),
            ("!while 1\n!endwhile", 0, "too long"),
            (
                "!function $f($x) !return $f($x)\nA : $f(1)",
                0,
                "nests too deep",
            ),
            (
                format!("{}{}", "!if 1\n".repeat(100), "!endif\n".repeat(100)).as_str(),
                64,
                "nests too deep",
            ),
            (
                "!$s = \"ab\"\n!while 1\n!$s = $s + $s\n!endwhile",
                2,
                "too long",
            ),
            ("!define A B\n!define B A\nA", 2, "without end"),
            (
                "!procedure $p($-->x)\nx\n!endprocedure",
                0,
                "`$-->x` is no parameter name",
            ),
            ("!define F(a-b) a-b", 0, "`a-b` is no parameter name"),
            (
                "!procedure $p()\nx\n!endprocedure\nA : $p()",
                3,
                "alone on its line",
            ),
            ("!assert 1 == 2 : \"wrong\"", 0, "wrong"),
            ("!$x = {\"a\": 1}", 0, "JSON"),
        ] {
            let err = declined(&format!("@startuml\n{source}\n@enduml"));
            assert_eq!(err.line, Some(line + 1), "{source}: {err:?}");
            assert!(err.reason.contains(part), "{source}: {}", err.reason);
        }
    }

    #[test]
    fn splits_arguments_and_finds_parentheses() {
        assert_eq!(
            split_args(" \"a, b\" , f(1, 2), 'c,d' "),
            ["\"a, b\"", "f(1, 2)", "'c,d'"]
        );
        assert_eq!(split_args("  "), Vec::<&str>::new());
        assert_eq!(whole_call("$p(\"x)\", 1)"), Some(("$p", "\"x)\", 1")));
        assert_eq!(whole_call("$p(1) more"), None);
        assert_eq!(closing_paren("f(don't)", 1), Some(7));
    }
}
