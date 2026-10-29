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

use crate::Declined;
use crate::annotations::Annotations;
use crate::color::Color;
use crate::common::Frame;
use crate::source::Line;
use crate::svg::{self, Align, Canvas, Layer, Stroke};
use crate::text::{self, Family, Font};

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

/// Parses the body of an `@startlatex` or `@startmath` diagram.
pub fn parse(lines: &[Line<'_>], notation: Notation) -> Result<Diagram, Declined> {
    let gathered = gather(lines, notation)?;
    if gathered.text.trim().is_empty() {
        let line = lines.last().map(|l| l.no).unwrap_or(0);
        return Err(Declined::at(line, "a formula needs an expression"));
    }
    let pairs = match notation {
        Notation::Latex => tex_pairs(&gathered.text),
        Notation::Ascii => bar_pairs(&gathered.text),
    };
    let mut cur = Cur {
        text: &gathered.text,
        at: 0,
        marks: &gathered.marks,
        depth: 0,
        notation,
        pairs: &pairs,
    };
    // A row to `Stop::End` reads all of the text or declines.
    let root = match notation {
        Notation::Latex => cur.tex_row(Stop::End)?,
        Notation::Ascii => cur.am_row(Stop::End)?,
    };
    Ok(Diagram {
        root,
        frame: gathered.frame,
    })
}

/// The SVG. The formula is black on a white page; skin parameters do not
/// recolor it.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    let mut canvas = Canvas::new("serif", false);
    let family = canvas.family();
    let notes = Annotations::new(
        &diagram.frame,
        [
            Font::new(TITLE, Color::BLACK).bold(),
            Font::new(10.0, MUTED),
            Font::new(10.0, MUTED),
            Font::new(TITLE, Color::BLACK),
        ],
        family,
    )?;
    let face = Face {
        size: DISPLAY,
        family,
        in_script: false,
    };
    let pack = layout(&diagram.root, face);
    let baseline = notes.top() + pack.height;
    paint(&mut canvas, &pack, 0.0, baseline, family);
    // An italic leans past the box the roman width reserved.
    canvas.include(pack.width + face.size * 0.08, baseline);
    notes.draw(&mut canvas);
    Ok(canvas.finish(MARGIN, Some(Color::WHITE)))
}

struct Gathered {
    text: String,
    marks: Vec<(usize, usize)>,
    frame: Frame,
}

/// Frame lines outside groups, and the expression with the source line of
/// each byte. A title inside a group that runs over several lines stays in
/// the expression.
fn gather(lines: &[Line<'_>], notation: Notation) -> Result<Gathered, Declined> {
    let mut frame = Frame::default();
    let mut text = String::new();
    let mut marks = Vec::new();
    let mut depth = 0_i32;
    let mut quote = false;
    let mut at = 0;
    while at < lines.len() {
        if depth == 0
            && !quote
            && let Some(next) = frame.line(lines, at)?
        {
            at = next;
            continue;
        }
        let line = lines[at];
        text.push_str(line.text);
        text.push('\n');
        marks.push((text.len(), line.no));
        scan(line.text, &mut depth, &mut quote, notation);
        at += 1;
    }
    Ok(Gathered { text, marks, frame })
}

/// Moves `depth` over one line. Only groups that must close count: TeX
/// braces and `\\left`…`\\right`, and AsciiMath brackets and quotes. A TeX
/// bracket need not close, so a frame line after `(a` is still one; a group
/// left open declines when the expression is read.
fn scan(line: &str, depth: &mut i32, quote: &mut bool, notation: Notation) {
    let tex = notation == Notation::Latex;
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if *quote {
            if c == '\\' {
                chars.next();
            } else if c == '"' {
                *quote = false;
            }
            continue;
        }
        if c == '\\' {
            let word = line[i + 1..]
                .bytes()
                .take_while(u8::is_ascii_alphabetic)
                .count();
            // A command, or one escaped character.
            for _ in 0..word.max(1) {
                chars.next();
            }
            let name = &line[i + 1..i + 1 + word];
            if tex && matches!(name, "left" | "right") {
                *depth += if name == "left" { 1 } else { -1 };
                // The delimiter belongs to the command.
                while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
                if chars.next().is_some_and(|(_, c)| c == '\\') {
                    chars.next();
                }
            }
            continue;
        }
        if c == '"' && !tex {
            *quote = true;
            continue;
        }
        match c {
            '{' => *depth += 1,
            '}' => *depth -= 1,
            '(' | '[' if !tex => *depth += 1,
            ')' | ']' if !tex => *depth -= 1,
            _ => {}
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    End,
    Brace,
    Bracket,
    /// Any closing bracket after the opening one it names: `[0, 1)` is one
    /// fence. A TeX `}` closes a group instead.
    Close(char),
    Bar,
    /// A TeX `\}`.
    TexBrace,
    /// A TeX `\right`.
    Right,
}

struct Cur<'a> {
    text: &'a str,
    at: usize,
    marks: &'a [(usize, usize)],
    depth: u32,
    notation: Notation,
    /// Where the brackets (TeX) or bars (AsciiMath) start that a closing
    /// one ends, in order.
    pairs: &'a [usize],
}

impl<'a> Cur<'a> {
    fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    fn line(&self) -> usize {
        let i = self.marks.partition_point(|(end, _)| *end <= self.at);
        self.marks
            .get(i)
            .or_else(|| self.marks.last())
            .map(|(_, no)| *no)
            .unwrap_or(0)
    }

    fn err(&self, reason: impl Into<String>) -> Declined {
        Declined::at(self.line(), reason)
    }

    fn enter(&mut self) -> Result<(), Declined> {
        self.depth += 1;
        if self.depth > NEST_LIMIT {
            Err(self.err("nested more than 100 levels deep is not supported yet"))
        } else {
            Ok(())
        }
    }

    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    fn starts_with(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.at += s.len();
            true
        } else {
            false
        }
    }

    fn at_right(&self) -> bool {
        let Some(after) = self.rest().strip_prefix("\\right") else {
            return false;
        };
        after
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphabetic())
    }

    fn at_stop(&self, stop: Stop) -> bool {
        match stop {
            Stop::End => false,
            Stop::Brace => self.peek() == Some('}'),
            Stop::Bracket => self.peek() == Some(']'),
            Stop::Close(_) => match self.peek() {
                Some(')' | ']') => true,
                Some('}') => self.notation == Notation::Ascii,
                _ => false,
            },
            Stop::Bar => self.peek() == Some('|') && ascii_name(self.rest()).is_none(),
            Stop::TexBrace => self.starts_with("\\}"),
            Stop::Right => self.at_right(),
        }
    }

    fn stray(&self, stop: Stop) -> Option<&'static str> {
        if self.at_stop(stop) {
            return None;
        }
        match self.peek() {
            // TeX pairs no brackets, so one alone is drawn.
            Some(')' | ']') if self.notation == Notation::Latex => None,
            Some(')') => Some("a `)` has no opening `(`"),
            Some(']') => Some("a `]` has no opening `[`"),
            Some('}') => Some("a `}` has no opening `{`"),
            _ if self.starts_with("\\}") => Some("a `\\}` has no `\\{`"),
            _ if self.at_right() => Some("a `\\right` has no `\\left`"),
            _ => None,
        }
    }

    /// Whether a closing bracket or bar ends the one that starts here.
    fn paired(&self) -> bool {
        self.pairs.binary_search(&self.at).is_ok()
    }

    fn read_word(&mut self) -> String {
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.bump();
        }
        self.text[start..self.at].to_string()
    }

    fn tex_row(&mut self, stop: Stop) -> Result<Node, Declined> {
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek().is_none() || self.at_stop(stop) {
                break;
            }
            if let Some(msg) = self.stray(stop) {
                return Err(self.err(msg));
            }
            let node = self.tex_atom()?;
            let node = if matches!(node, Node::Space(_)) {
                node
            } else {
                self.attach_scripts(node, true)?
            };
            items.push(node);
        }
        Ok(Node::Row(items))
    }

    fn tex_atom(&mut self) -> Result<Node, Declined> {
        self.skip_ws();
        match self.peek() {
            None => Err(self.err("a formula needs an expression")),
            Some('\\') => {
                self.bump();
                self.tex_command()
            }
            Some('{') => {
                self.bump();
                self.tex_group(Stop::Brace)
            }
            Some(c @ ('(' | '[')) if self.paired() => {
                self.bump();
                let open = if c == '(' {
                    Fence::Paren
                } else {
                    Fence::Bracket
                };
                self.tex_fence(open, Stop::Close(c))
            }
            Some(c @ ('(' | '[')) => {
                self.bump();
                Ok(plain(&c.to_string(), Atom::Open, true))
            }
            Some(c @ (')' | ']')) => {
                self.bump();
                Ok(plain(&c.to_string(), Atom::Close, true))
            }
            Some(c) if c.is_ascii_digit() || self.decimal() => Ok(self.number()),
            Some(c) if c.is_ascii_alphabetic() => {
                let c = self.bump().unwrap_or(c);
                Ok(plain(&c.to_string(), Atom::Ord, false))
            }
            Some(c) => self.tex_char(c),
        }
    }

    fn decimal(&self) -> bool {
        let mut chars = self.rest().chars();
        chars.next() == Some('.') && chars.next().is_some_and(|c| c.is_ascii_digit())
    }

    fn tex_char(&mut self, c: char) -> Result<Node, Declined> {
        self.bump();
        match c {
            '+' => Ok(plain("+", Atom::Bin, true)),
            '-' => Ok(plain("−", Atom::Bin, true)),
            '*' => Ok(plain("∗", Atom::Bin, true)),
            '/' | '|' => Ok(plain(&c.to_string(), Atom::Ord, true)),
            '=' | '<' | '>' | ':' => Ok(plain(&c.to_string(), Atom::Rel, true)),
            ',' | ';' | '.' => Ok(plain(&c.to_string(), Atom::Punct, true)),
            '!' => Ok(plain("!", Atom::Ord, true)),
            '\'' => Ok(plain("′", Atom::Ord, true)),
            '~' => Ok(Node::Space(0.33)),
            '$' => Err(self.err("`$` is not supported yet")),
            '%' => Err(self.err("`%` is not supported yet")),
            '&' => Err(self.err("`&` is not supported yet")),
            _ => Err(self.err(format!("`{c}` is not supported yet"))),
        }
    }

    fn tex_command(&mut self) -> Result<Node, Declined> {
        if self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            let name = self.read_word();
            return self.tex_named(&name);
        }
        match self.bump() {
            Some(c) => self.one_char(c),
            None => Err(self.err("a `\\` needs a command")),
        }
    }

    fn tex_named(&mut self, name: &str) -> Result<Node, Declined> {
        match name {
            "frac" => self.tex_frac(),
            "sqrt" => self.tex_sqrt(),
            "left" => self.tex_left(),
            "right" => Err(self.err("a `\\right` has no `\\left`")),
            "mathrm" | "text" => self.tex_upright(name),
            "quad" => Ok(Node::Space(1.0)),
            _ => symbol(name)
                .map(from_sym)
                .ok_or_else(|| self.err(format!("`\\{name}` is not supported yet"))),
        }
    }

    fn one_char(&mut self, c: char) -> Result<Node, Declined> {
        match c {
            ',' => Ok(Node::Space(THIN)),
            ':' => Ok(Node::Space(MED)),
            ';' => Ok(Node::Space(THICK)),
            '!' => Ok(Node::Space(-THIN)),
            ' ' => Ok(Node::Space(0.33)),
            '{' => self.tex_fence(Fence::Brace, Stop::TexBrace),
            '|' => Ok(plain("‖", Atom::Ord, true)),
            '\\' => Err(self.err("a line break is not supported yet")),
            _ => Err(self.err(format!("`\\{c}` is not supported yet"))),
        }
    }

    fn tex_group(&mut self, stop: Stop) -> Result<Node, Declined> {
        let line = self.line();
        self.enter()?;
        let body = self.tex_row(stop)?;
        if !self.at_stop(stop) {
            return Err(Declined::at(line, unclosed(stop)));
        }
        self.consume_stop(stop);
        self.leave();
        Ok(body)
    }

    fn tex_fence(&mut self, open: Fence, stop: Stop) -> Result<Node, Declined> {
        let line = self.line();
        self.enter()?;
        let body = self.tex_row(stop)?;
        if !self.at_stop(stop) {
            return Err(Declined::at(line, unclosed(stop)));
        }
        let close = self.consume_stop(stop);
        self.leave();
        Ok(Node::Fence {
            open,
            body: Box::new(body),
            close,
        })
    }

    fn tex_frac(&mut self) -> Result<Node, Declined> {
        self.enter()?;
        let num = self.tex_required("\\frac")?;
        let den = self.tex_required("\\frac")?;
        self.leave();
        Ok(Node::Frac(Box::new(num), Box::new(den)))
    }

    fn tex_sqrt(&mut self) -> Result<Node, Declined> {
        self.skip_ws();
        let index = if self.peek() == Some('[') {
            let line = self.line();
            self.bump();
            self.enter()?;
            let idx = self.tex_row(Stop::Bracket)?;
            if self.peek() != Some(']') {
                return Err(Declined::at(line, "a `\\sqrt` index has no `]`"));
            }
            self.bump();
            self.leave();
            Some(Box::new(idx))
        } else {
            None
        };
        self.enter()?;
        let body = self.tex_required("\\sqrt")?;
        self.leave();
        Ok(Node::Sqrt {
            index,
            body: Box::new(body),
        })
    }

    fn tex_left(&mut self) -> Result<Node, Declined> {
        let line = self.line();
        let open = self.delimiter()?;
        self.enter()?;
        let body = self.tex_row(Stop::Right)?;
        if !self.at_right() {
            return Err(Declined::at(line, "a `\\left` has no `\\right`"));
        }
        self.eat("\\right");
        let close = self.delimiter()?;
        self.leave();
        Ok(Node::Fence {
            open,
            body: Box::new(body),
            close,
        })
    }

    fn delimiter(&mut self) -> Result<Fence, Declined> {
        self.skip_ws();
        let line = self.line();
        match self.bump() {
            Some('(' | ')') => Ok(Fence::Paren),
            Some('[' | ']') => Ok(Fence::Bracket),
            Some('|') => Ok(Fence::Bar),
            Some('.') => Ok(Fence::None),
            Some('\\') => match self.bump() {
                Some('{' | '}') => Ok(Fence::Brace),
                Some('|') => Ok(Fence::Double),
                Some(c) => Err(Declined::at(
                    line,
                    format!("`\\{c}` is not a delimiter this formula draws"),
                )),
                None => Err(Declined::at(line, "a `\\left` needs a delimiter")),
            },
            Some(c) => Err(Declined::at(
                line,
                format!("`{c}` is not a delimiter this formula draws"),
            )),
            None => Err(self.err("a delimiter is missing")),
        }
    }

    fn tex_required(&mut self, what: &str) -> Result<Node, Declined> {
        self.skip_ws();
        if self.peek().is_none()
            || self.stray(Stop::End).is_some()
            || matches!(self.peek(), Some('^' | '_'))
        {
            return Err(self.err(format!("`{what}` needs an argument")));
        }
        self.tex_token()
    }

    /// An undelimited argument or script is one token, so `\frac12` is 1
    /// over 2 and `x^23` is x² 3. A group still holds a whole number.
    fn tex_token(&mut self) -> Result<Node, Declined> {
        match self.peek() {
            Some(c) if c.is_ascii_digit() => {
                self.bump();
                Ok(plain(&c.to_string(), Atom::Ord, true))
            }
            Some('.') => self.tex_char('.'),
            _ => self.tex_atom(),
        }
    }

    /// `\mathrm` is math in upright letters, and `\text` is text.
    fn tex_upright(&mut self, name: &str) -> Result<Node, Declined> {
        self.skip_ws();
        if name == "text" && self.peek() == Some('{') {
            let text = self.literal_group()?;
            return Ok(if text.is_empty() {
                Node::Row(Vec::new())
            } else {
                plain(&text, Atom::Ord, true)
            });
        }
        self.enter()?;
        let body = self.tex_required(name)?;
        self.leave();
        Ok(force_upright(body))
    }

    fn literal_group(&mut self) -> Result<String, Declined> {
        let line = self.line();
        self.bump();
        let mut out = String::new();
        let mut depth = 1_u32;
        while let Some(c) = self.bump() {
            if c == '\\' {
                match self.bump() {
                    Some(n @ ('\\' | '{' | '}' | ' ')) => out.push(n),
                    Some(n) => {
                        return Err(self.err(format!("`\\{n}` inside text is not supported yet")));
                    }
                    None => break,
                }
                continue;
            }
            // Braces group and are not drawn; `\{` draws one.
            if c == '{' {
                depth += 1;
                continue;
            }
            if c == '}' {
                depth -= 1;
                if depth == 0 {
                    return Ok(out);
                }
                continue;
            }
            if matches!(c, '^' | '_' | '$') {
                return Err(self.err(format!("`{c}` inside text is not supported yet")));
            }
            out.push(if c == '\n' { ' ' } else { c });
        }
        Err(Declined::at(line, "a `{` has no closing `}`"))
    }

    fn am_row(&mut self, stop: Stop) -> Result<Node, Declined> {
        let mut items = Vec::new();
        // Whether the last item is a fraction a `/` made.
        let mut fraction = false;
        loop {
            self.skip_ws();
            if self.peek().is_none() || self.at_stop(stop) {
                break;
            }
            if let Some(msg) = self.stray(stop) {
                return Err(self.err(msg));
            }
            if self.am_char('/') && fraction {
                self.bump();
                items.push(plain("/", Atom::Ord, true));
                fraction = false;
                continue;
            }
            if self.am_char('/') {
                let Some(num) = numerator(&mut items) else {
                    return Err(self.err("a fraction needs a numerator"));
                };
                self.bump();
                let den = self.denominator(stop)?;
                items.push(Node::Frac(Box::new(num), Box::new(den)));
                fraction = true;
                continue;
            }
            items.push(self.am_one()?);
            fraction = false;
        }
        Ok(Node::Row(items))
    }

    fn am_one(&mut self) -> Result<Node, Declined> {
        let node = self.am_token()?;
        if matches!(node, Node::Space(_)) {
            return Ok(node);
        }
        self.attach_scripts(node, false)
    }

    fn am_token(&mut self) -> Result<Node, Declined> {
        self.skip_ws();
        if let Some((name, word)) = ascii_name(self.rest()) {
            self.at += name.len();
            return self.am_word(name, word);
        }
        let Some(c) = self.peek() else {
            return Err(self.err("a formula needs an expression"));
        };
        if c.is_ascii_digit() || self.decimal() {
            return Ok(self.number());
        }
        match c {
            '\\' => {
                self.bump();
                self.tex_command()
            }
            '(' => self.am_fence(Fence::Paren, Stop::Close(c)),
            '[' => self.am_fence(Fence::Bracket, Stop::Close(c)),
            '{' => self.am_fence(Fence::Brace, Stop::Close(c)),
            '|' if self.paired() => self.am_fence(Fence::Bar, Stop::Bar),
            '"' => self.quoted(),
            _ => {
                self.bump();
                match c {
                    _ if c.is_ascii_alphabetic() => Ok(plain(&c.to_string(), Atom::Ord, false)),
                    '+' => Ok(plain("+", Atom::Bin, true)),
                    '-' => Ok(plain("−", Atom::Bin, true)),
                    '=' | '<' | '>' | ':' => Ok(plain(&c.to_string(), Atom::Rel, true)),
                    ',' | ';' | '.' => Ok(plain(&c.to_string(), Atom::Punct, true)),
                    // A bar no second bar closes, as in `P(A|B)`.
                    '!' | '|' => Ok(plain(&c.to_string(), Atom::Ord, true)),
                    '\'' => Ok(plain("′", Atom::Ord, true)),
                    _ => Err(self.err(format!("`{c}` is not supported yet"))),
                }
            }
        }
    }

    fn am_word(&mut self, name: &str, word: Word) -> Result<Node, Declined> {
        Ok(match word {
            Word::Symbol(sym) => from_sym(sym),
            Word::Function => Node::Glyph(Glyph {
                limits: function_limits(name),
                ..glyph(name, Atom::Op, true)
            }),
            Word::Space(width) => Node::Space(width),
            Word::Spaced(text) => Node::Row(vec![
                Node::Space(1.0),
                plain(text, Atom::Ord, true),
                Node::Space(1.0),
            ]),
            Word::Differential(var) => Node::Row(vec![
                plain("d", Atom::Ord, false),
                plain(var, Atom::Ord, false),
            ]),
            Word::Construct => return self.am_construct(name),
            Word::Declined => return Err(self.err(format!("`{name}` is not supported yet"))),
        })
    }

    fn am_construct(&mut self, name: &str) -> Result<Node, Declined> {
        if matches!(name, "text" | "mbox") {
            return self.am_text(name);
        }
        self.enter()?;
        let node = match name {
            "sqrt" => Node::Sqrt {
                index: None,
                body: Box::new(self.am_argument(name)?),
            },
            "root" => {
                let index = self.am_argument(name)?;
                Node::Sqrt {
                    index: Some(Box::new(index)),
                    body: Box::new(self.am_argument(name)?),
                }
            }
            "frac" => {
                let num = self.am_argument(name)?;
                Node::Frac(Box::new(num), Box::new(self.am_argument(name)?))
            }
            "abs" | "norm" => {
                let fence = if name == "abs" {
                    Fence::Bar
                } else {
                    Fence::Double
                };
                Node::Fence {
                    open: fence,
                    body: Box::new(self.am_argument(name)?),
                    close: fence,
                }
            }
            _ => return Err(self.err(format!("`{name}` is not supported yet"))),
        };
        self.leave();
        Ok(node)
    }

    /// One item without its scripts, which apply to the whole construct
    /// (`sqrt x^2` is (√x)²), and without its brackets.
    fn am_argument(&mut self, what: &str) -> Result<Node, Declined> {
        if self.at_end(Stop::End) {
            return Err(self.err(format!("a `{what}` needs an argument")));
        }
        Ok(unbracket(self.am_token()?))
    }

    /// Upright text up to the first closing bracket of the kind that opens
    /// it, as AsciiMath reads `text(m/s)`.
    fn am_text(&mut self, name: &str) -> Result<Node, Declined> {
        self.skip_ws();
        let line = self.line();
        let close = match self.peek() {
            Some('(') => ')',
            Some('[') => ']',
            Some('{') => '}',
            _ => return Err(self.err(format!("a `{name}` needs a bracket"))),
        };
        self.bump();
        let Some(len) = self.rest().find(close) else {
            return Err(Declined::at(
                line,
                format!("a `{name}` has no closing `{close}`"),
            ));
        };
        let text = self.rest()[..len].replace('\n', " ");
        self.at += len + 1;
        Ok(plain(&text, Atom::Ord, true))
    }

    /// The item after an AsciiMath `/`, with a minus before it or the
    /// function it is the argument of, as in `1/-x` and `1/sin x`.
    fn denominator(&mut self, stop: Stop) -> Result<Node, Declined> {
        let mut den = Vec::new();
        let mut item = self.next_item(stop)?;
        if is_minus(&item) {
            den.push(item);
            item = self.next_item(stop)?;
        }
        let takes = !self.at_end(stop)
            && !self.am_char('/')
            && (is_function(&item) || (is_letter_function(&item) && self.am_char('(')));
        if takes {
            den.push(item);
            item = self.next_item(stop)?;
        }
        if !is_operand(&item) {
            return Err(self.err("a fraction needs a denominator"));
        }
        den.push(item);
        Ok(unbracket(join(den)))
    }

    fn next_item(&mut self, stop: Stop) -> Result<Node, Declined> {
        if self.at_end(stop) {
            return Err(self.err("a fraction needs a denominator"));
        }
        self.am_one()
    }

    fn at_end(&mut self, stop: Stop) -> bool {
        self.skip_ws();
        self.peek().is_none() || self.at_stop(stop) || self.stray(stop).is_some()
    }

    /// Whether the next character is `c` and does not start a longer
    /// AsciiMath name, as `/` does in `//`.
    fn am_char(&self, c: char) -> bool {
        self.peek() == Some(c) && ascii_name(self.rest()).is_none()
    }

    fn am_fence(&mut self, open: Fence, stop: Stop) -> Result<Node, Declined> {
        let line = self.line();
        self.bump();
        self.enter()?;
        let body = self.am_row(stop)?;
        if !self.at_stop(stop) {
            return Err(Declined::at(line, unclosed(stop)));
        }
        if matches!(&body, Node::Row(items) if is_matrix(items)) {
            return Err(Declined::at(line, "a matrix is not supported yet"));
        }
        let close = self.consume_stop(stop);
        self.leave();
        Ok(Node::Fence {
            open,
            body: Box::new(body),
            close,
        })
    }

    fn quoted(&mut self) -> Result<Node, Declined> {
        let line = self.line();
        self.bump();
        let start = self.at;
        while self.peek().is_some_and(|c| c != '"') {
            self.bump();
        }
        if self.peek() != Some('"') {
            return Err(Declined::at(line, "a quote has no closing `\"`"));
        }
        let text = self.text[start..self.at].replace('\n', " ");
        self.bump();
        Ok(plain(&text, Atom::Ord, true))
    }

    /// `tex` uses a TeX script body (one atom), and AsciiMath one item
    /// without its brackets.
    fn attach_scripts(&mut self, base: Node, tex: bool) -> Result<Node, Declined> {
        let mut sup = None;
        let mut sub = None;
        loop {
            self.skip_ws();
            let up = match self.peek() {
                Some('^') => true,
                Some('_') => false,
                _ => break,
            };
            // `^^` and `__|` are AsciiMath names, not scripts.
            if !tex && ascii_name(self.rest()).is_some() {
                break;
            }
            if up && sup.is_some() {
                return Err(self.err("a second superscript is not supported yet"));
            }
            if !up && sub.is_some() {
                return Err(self.err("a second subscript is not supported yet"));
            }
            self.bump();
            let body = self.script_body(tex)?;
            if up {
                sup = Some(body);
            } else {
                sub = Some(body);
            }
        }
        self.absorb_primes(&mut sup)?;
        if sup.is_none() && sub.is_none() {
            return Ok(base);
        }
        let limits = limits_of(&base);
        Ok(Node::Script {
            base: Box::new(base),
            sup: sup.map(Box::new),
            sub: sub.map(Box::new),
            limits,
        })
    }

    fn script_body(&mut self, tex: bool) -> Result<Node, Declined> {
        self.skip_ws();
        if self.peek().is_none()
            || self.stray(Stop::End).is_some()
            || matches!(self.peek(), Some('^' | '_'))
        {
            return Err(self.err("a script needs an expression"));
        }
        self.enter()?;
        let node = if tex {
            self.tex_token()?
        } else {
            self.am_script()?
        };
        self.leave();
        Ok(node)
    }

    /// A minus takes the item after it with that item's scripts, as
    /// AsciiMath reads `e^-x^2` as e^(−x²).
    fn am_script(&mut self) -> Result<Node, Declined> {
        let item = self.am_token()?;
        if !is_minus(&item) {
            return Ok(unbracket(item));
        }
        if self.at_end(Stop::End) {
            return Err(self.err("a script needs an expression"));
        }
        Ok(Node::Row(vec![item, self.am_one()?]))
    }

    fn absorb_primes(&mut self, sup: &mut Option<Node>) -> Result<(), Declined> {
        self.skip_ws();
        if self.peek() != Some('\'') {
            return Ok(());
        }
        let mut text = String::new();
        while self.peek() == Some('\'') {
            self.bump();
            text.push('′');
        }
        let prime = plain(&text, Atom::Ord, true);
        *sup = Some(match sup.take() {
            None => prime,
            Some(existing) => Node::Row(vec![existing, prime]),
        });
        Ok(())
    }

    fn number(&mut self) -> Node {
        let start = self.at;
        let mut dot = false;
        if self.peek() == Some('.') {
            dot = true;
            self.bump();
        }
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.bump();
            } else if c == '.' && !dot {
                dot = true;
                self.bump();
            } else {
                break;
            }
        }
        if self.text[start..self.at].ends_with('.')
            && self.text[start..self.at]
                .chars()
                .any(|c| c.is_ascii_digit())
        {
            self.at -= 1;
        }
        plain(&self.text[start..self.at], Atom::Ord, true)
    }

    /// Takes the stop and says which fence it draws.
    fn consume_stop(&mut self, stop: Stop) -> Fence {
        match stop {
            Stop::Brace | Stop::Bracket | Stop::Bar | Stop::Close(_) => match self.bump() {
                Some(']') => Fence::Bracket,
                Some('}') => Fence::Brace,
                Some('|') => Fence::Bar,
                _ => Fence::Paren,
            },
            Stop::TexBrace => {
                self.eat("\\}");
                Fence::Brace
            }
            Stop::Right | Stop::End => Fence::None,
        }
    }
}

fn unclosed(stop: Stop) -> String {
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

fn plain(text: &str, atom: Atom, upright: bool) -> Node {
    Node::Glyph(glyph(text, atom, upright))
}

fn glyph(text: &str, atom: Atom, upright: bool) -> Glyph {
    Glyph {
        text: text.to_string(),
        atom,
        upright,
        scale: 1.0,
        limits: Limits::Side,
    }
}

fn from_sym(sym: Sym) -> Node {
    Node::Glyph(Glyph {
        text: sym.text.to_string(),
        atom: sym.atom,
        upright: sym.upright,
        scale: sym.scale,
        limits: sym.limits,
    })
}

fn join(mut items: Vec<Node>) -> Node {
    if items.len() == 1 {
        items.remove(0)
    } else {
        Node::Row(items)
    }
}

fn limits_of(node: &Node) -> Limits {
    match node {
        Node::Glyph(g) => g.limits,
        _ => Limits::Side,
    }
}

/// Latin letters turn upright; a lowercase Greek letter stays italic, as
/// in TeX.
fn force_upright(node: Node) -> Node {
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
fn numerator(items: &mut Vec<Node>) -> Option<Node> {
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

fn is_operand(node: &Node) -> bool {
    !matches!(node, Node::Space(_))
        && !matches!(
            atom_class(node),
            Atom::Bin | Atom::Rel | Atom::Punct | Atom::Open | Atom::Close
        )
}

fn is_minus(node: &Node) -> bool {
    matches!(node, Node::Glyph(g) if g.text == "−" && g.atom == Atom::Bin)
}

fn is_paren(node: &Node) -> bool {
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
fn is_function(node: &Node) -> bool {
    match node {
        Node::Glyph(g) => g.atom == Atom::Op && ASCII_FUNCTIONS.contains(&g.text.as_str()),
        Node::Script { base, .. } => is_function(base),
        _ => false,
    }
}

/// `f` and `g`, which AsciiMath reads as functions of a parenthesis after
/// them only.
fn is_letter_function(node: &Node) -> bool {
    match node {
        Node::Glyph(g) => !g.upright && matches!(g.text.as_str(), "f" | "g"),
        Node::Script { base, .. } => is_letter_function(base),
        _ => false,
    }
}

/// AsciiMath drops the brackets around either side of a `/`, around a
/// script and around the argument of `sqrt`.
fn unbracket(node: Node) -> Node {
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
fn is_matrix(items: &[Node]) -> bool {
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

/// What an AsciiMath name stands for.
enum Word {
    Symbol(Sym),
    /// A function such as `sin`, which takes the next item as its argument.
    Function,
    Space(f32),
    /// `and`, `or` and `if`, with a quad on either side.
    Spaced(&'static str),
    /// `dx`, one item for a `/`, so `dy/dx` is a fraction of the two.
    Differential(&'static str),
    /// A name with arguments: `sqrt`, `root`, `frac`, `text`, `mbox`,
    /// `abs` and `norm`.
    Construct,
    /// A name this module does not draw yet. It declines instead of
    /// drawing the letters, so the Java PlantUML draws the sheet.
    Declined,
}

/// The length of the longest name, `twoheadrightarrowtail`.
const LONGEST_NAME: usize = 21;

/// The longest AsciiMath name `rest` starts with. AsciiMath reads names
/// this way, so `sinx` is sin x and `xin` is ξ n; a letter that starts no
/// name is a variable.
fn ascii_name(rest: &str) -> Option<(&str, Word)> {
    let run = rest
        .char_indices()
        .find(|(_, c)| c.is_whitespace() || c.is_ascii_digit())
        .map_or(rest.len(), |(at, _)| at);
    (1..=run.min(LONGEST_NAME))
        .rev()
        .filter(|&end| rest.is_char_boundary(end))
        .find_map(|end| ascii_word(&rest[..end]).map(|word| (&rest[..end], word)))
}

fn ascii_word(name: &str) -> Option<Word> {
    if let Some(sym) = ascii_symbol(name) {
        return Some(Word::Symbol(sym));
    }
    Some(match name {
        "sqrt" | "root" | "frac" | "text" | "mbox" | "abs" | "norm" => Word::Construct,
        "quad" => Word::Space(1.0),
        "qquad" => Word::Space(2.0),
        "and" => Word::Spaced("and"),
        "or" => Word::Spaced("or"),
        "if" => Word::Spaced("if"),
        "dx" => Word::Differential("x"),
        "dy" => Word::Differential("y"),
        "dz" => Word::Differential("z"),
        "dt" => Word::Differential("t"),
        _ if ASCII_FUNCTIONS.contains(&name) => Word::Function,
        _ if ASCII_DECLINED.contains(&name) => Word::Declined,
        _ => return None,
    })
}

const ASCII_GREEK: &[&str] = &[
    "alpha",
    "beta",
    "chi",
    "delta",
    "Delta",
    "epsilon",
    "varepsilon",
    "eta",
    "gamma",
    "Gamma",
    "iota",
    "kappa",
    "lambda",
    "Lambda",
    "mu",
    "nu",
    "omega",
    "Omega",
    "phi",
    "varphi",
    "Phi",
    "pi",
    "Pi",
    "psi",
    "Psi",
    "rho",
    "sigma",
    "Sigma",
    "tau",
    "theta",
    "vartheta",
    "Theta",
    "upsilon",
    "xi",
    "Xi",
    "zeta",
];

const ASCII_FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "cot", "coth",
    "sech", "csch", "sec", "csc", "log", "ln", "det", "exp", "gcd", "lcm",
];

/// Where the TeX `(` and `[` start that a `)` or `]` closes on their level.
/// TeX pairs no brackets, so `[0, 1)`, `]0, 1[` and `(a` are all
/// formulas; a pair is one fence, which grows around a tall subformula.
fn tex_pairs(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    // `Some` is a bracket that may pair, `None` a group it cannot leave.
    let mut open: Vec<Option<usize>> = Vec::new();
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        i += 1;
        match c {
            b'\\' => {
                let word = bytes[i..]
                    .iter()
                    .take_while(|b| b.is_ascii_alphabetic())
                    .count();
                let name = &text[i..i + word];
                i += word.max(1);
                match name {
                    "" if bytes.get(i - 1) == Some(&b'{') => open.push(None),
                    "" if bytes.get(i - 1) == Some(&b'}') => close_group(&mut open),
                    "left" | "right" => {
                        // The delimiter belongs to the command.
                        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                            i += 1;
                        }
                        i += if bytes.get(i) == Some(&b'\\') { 2 } else { 1 };
                        if name == "left" {
                            open.push(None);
                        } else {
                            close_group(&mut open);
                        }
                    }
                    _ => {}
                }
            }
            b'(' | b'[' => open.push(Some(i - 1)),
            b')' | b']' => {
                if let Some(Some(at)) = open.last() {
                    pairs.push(*at);
                    open.pop();
                }
            }
            b'{' => open.push(None),
            b'}' => close_group(&mut open),
            _ => {}
        }
    }
    pairs.sort_unstable();
    pairs
}

/// Where the AsciiMath `|` start that a second `|` closes on their level;
/// any other bar is drawn as it is, as in `P(A|B)` and `{x | x > 0}`.
fn bar_pairs(text: &str) -> Vec<usize> {
    // `Some` is a bar that may pair, `None` a bracket it cannot leave.
    let mut open: Vec<Option<usize>> = Vec::new();
    let mut pairs = Vec::new();
    let mut at = 0;
    while let Some(c) = text[at..].chars().next() {
        // A name such as `|->` is not a bar; a name of letters holds none.
        if !c.is_alphanumeric()
            && !c.is_whitespace()
            && let Some((name, _)) = ascii_name(&text[at..])
        {
            at += name.len();
            continue;
        }
        match c {
            '(' | '[' | '{' => open.push(None),
            ')' | ']' | '}' => close_group(&mut open),
            '|' => {
                if let Some(Some(start)) = open.last() {
                    pairs.push(*start);
                    open.pop();
                } else {
                    open.push(Some(at));
                }
            }
            '"' => {
                if let Some(end) = text[at + 1..].find('"') {
                    at += end + 1;
                }
            }
            _ => {}
        }
        at += c.len_utf8();
    }
    pairs.sort_unstable();
    pairs
}

/// Ends the innermost group, and with it the openers inside that found no
/// partner.
fn close_group(open: &mut Vec<Option<usize>>) {
    while let Some(Some(_)) = open.pop() {}
}

/// Names PlantUML's AsciiMath reads that this module does not draw: accents,
/// fonts, colors, angle and invisible brackets, the capitalized functions
/// and the few the Java PlantUML cannot draw either.
const ASCII_DECLINED: &[&str] = &[
    "stackrel",
    "overset",
    "underset",
    "color",
    "cancel",
    "Sqrt",
    "hat",
    "bar",
    "overline",
    "vec",
    "tilde",
    "dot",
    "ddot",
    "overarc",
    "overparen",
    "ul",
    "underline",
    "ubrace",
    "underbrace",
    "obrace",
    "overbrace",
    "bb",
    "mathbf",
    "sf",
    "mathsf",
    "bbb",
    "mathbb",
    "cc",
    "mathcal",
    "tt",
    "mathtt",
    "fr",
    "mathfrak",
    "floor",
    "ceil",
    "Abs",
    "Lim",
    "Sin",
    "Cos",
    "Tan",
    "Arcsin",
    "Arccos",
    "Arctan",
    "Sinh",
    "Cosh",
    "Tanh",
    "Cot",
    "Sec",
    "Csc",
    "Log",
    "Ln",
    "lub",
    "glb",
    "-lt",
    ">->>",
    "twoheadrightarrowtail",
    "(:",
    ":)",
    "<<",
    ">>",
    "langle",
    "rangle",
    "{:",
    ":}",
    "|:",
    ":|",
    ":|:",
    "lbrace",
    "rbrace",
    "left(",
    "right)",
    "left[",
    "right]",
];

/// The AsciiMath names drawn as one symbol, with the TeX names AsciiMath
/// reads too.
fn ascii_symbol(name: &str) -> Option<Sym> {
    if ASCII_GREEK.contains(&name)
        || matches!(
            name,
            "sum" | "prod" | "int" | "oint" | "lim" | "min" | "max"
        )
    {
        return symbol(name);
    }
    match name {
        "epsi" => return symbol("epsilon"),
        "lamda" => return symbol("lambda"),
        "Lamda" => return symbol("Lambda"),
        "^^^" | "bigwedge" => return Some(big("⋀", Limits::Above)),
        "vvv" | "bigvee" => return Some(big("⋁", Limits::Above)),
        "nnn" | "bigcap" => return Some(big("⋂", Limits::Above)),
        "uuu" | "bigcup" => return Some(big("⋃", Limits::Above)),
        _ => {}
    }
    ascii_operator(name)
        .or_else(|| ascii_relation(name))
        .or_else(|| ascii_other(name))
        .map(|(text, atom)| sym(text, atom, true))
}

fn ascii_operator(name: &str) -> Option<(&'static str, Atom)> {
    let text = match name {
        "*" | "cdot" => "·",
        "**" | "ast" => "∗",
        "***" | "star" => "⋆",
        "setminus" => "∖",
        "xx" | "times" => "×",
        "|><" | "ltimes" => "⋉",
        "><|" | "rtimes" => "⋊",
        "-:" | "div" | "divide" => "÷",
        "@" | "circ" => "∘",
        "o+" | "oplus" => "⊕",
        "ox" | "otimes" => "⊗",
        "o." | "odot" => "⊙",
        "^^" | "wedge" => "∧",
        "vv" | "vee" => "∨",
        "nn" | "cap" => "∩",
        "uu" | "cup" => "∪",
        "+-" | "pm" => "±",
        "-+" | "mp" => "∓",
        "diamond" => "⋄",
        _ => return None,
    };
    Some((text, Atom::Bin))
}

fn ascii_relation(name: &str) -> Option<(&'static str, Atom)> {
    let text = match name {
        "!=" | "ne" => "≠",
        ":=" => ":=",
        "lt" => "<",
        "gt" => ">",
        "<=" | "lt=" | "le" | "leq" => "≤",
        ">=" | "gt=" | "ge" | "geq" => "≥",
        "mlt" | "ll" => "≪",
        "mgt" | "gg" => "≫",
        "-<" | "prec" => "≺",
        ">-" | "succ" => "≻",
        "-<=" | "preceq" => "⪯",
        ">-=" | "succeq" => "⪰",
        "in" => "∈",
        "!in" | "notin" => "∉",
        "sub" | "subset" => "⊂",
        "sup" | "supset" => "⊃",
        "sube" | "subseteq" => "⊆",
        "supe" | "supseteq" => "⊇",
        "-=" | "equiv" => "≡",
        "~=" | "cong" => "≅",
        "~" | "sim" => "∼",
        "~~" | "approx" => "≈",
        "prop" | "propto" => "∝",
        "|><|" | "bowtie" => "⋈",
        "=>" | "implies" | "Rightarrow" | "rArr" => "⇒",
        "<=>" | "iff" | "Leftrightarrow" | "hArr" => "⇔",
        "|--" | "vdash" => "⊢",
        "|==" | "models" => "⊨",
        "frown" => "⌢",
        "uarr" | "uparrow" => "↑",
        "darr" | "downarrow" => "↓",
        "rarr" | "rightarrow" | "->" | "to" => "→",
        ">->" | "rightarrowtail" => "↣",
        "->>" | "twoheadrightarrow" => "↠",
        "|->" | "mapsto" => "↦",
        "larr" | "leftarrow" => "←",
        "harr" | "leftrightarrow" => "↔",
        "uArr" | "Uparrow" => "⇑",
        "dArr" | "Downarrow" => "⇓",
        "lArr" | "Leftarrow" => "⇐",
        _ => return None,
    };
    Some((text, Atom::Rel))
}

fn ascii_other(name: &str) -> Option<(&'static str, Atom)> {
    Some(match name {
        "//" => ("/", Atom::Ord),
        "\\\\" | "backslash" => ("\\", Atom::Ord),
        "not" | "neg" => ("¬", Atom::Ord),
        "AA" | "forall" => ("∀", Atom::Ord),
        "EE" | "exists" => ("∃", Atom::Ord),
        "_|_" | "bot" => ("⊥", Atom::Ord),
        "TT" | "top" => ("⊤", Atom::Ord),
        "del" | "partial" => ("∂", Atom::Ord),
        "grad" | "nabla" => ("∇", Atom::Ord),
        "O/" | "emptyset" => ("∅", Atom::Ord),
        "oo" | "infty" => ("∞", Atom::Ord),
        "aleph" => ("ℵ", Atom::Ord),
        "..." | "ldots" => ("…", Atom::Ord),
        "cdots" => ("⋯", Atom::Ord),
        "vdots" => ("⋮", Atom::Ord),
        "ddots" => ("⋱", Atom::Ord),
        ":." | "therefore" => ("∴", Atom::Ord),
        ":'" | "because" => ("∵", Atom::Ord),
        "/_" | "angle" => ("∠", Atom::Ord),
        "/_\\" | "triangle" => ("△", Atom::Ord),
        "square" | "boxempty" => ("□", Atom::Ord),
        "%" => ("%", Atom::Ord),
        "prime" => ("′", Atom::Ord),
        "CC" => ("ℂ", Atom::Ord),
        "NN" => ("ℕ", Atom::Ord),
        "QQ" => ("ℚ", Atom::Ord),
        "RR" => ("ℝ", Atom::Ord),
        "ZZ" => ("ℤ", Atom::Ord),
        "|__" | "lfloor" => ("⌊", Atom::Open),
        "__|" | "rfloor" => ("⌋", Atom::Close),
        "|~" | "lceil" | "lceiling" => ("⌈", Atom::Open),
        "~|" | "rceil" | "rceiling" => ("⌉", Atom::Close),
        "dim" => ("dim", Atom::Op),
        "mod" => ("mod", Atom::Ord),
        _ => return None,
    })
}

fn atom_class(node: &Node) -> Atom {
    match node {
        Node::Glyph(g) => g.atom,
        Node::Script { base, .. } => atom_class(base),
        _ => Atom::Ord,
    }
}

struct Sym {
    text: &'static str,
    atom: Atom,
    upright: bool,
    scale: f32,
    limits: Limits,
}

fn sym(text: &'static str, atom: Atom, upright: bool) -> Sym {
    Sym {
        text,
        atom,
        upright,
        scale: 1.0,
        limits: Limits::Side,
    }
}

fn big(text: &'static str, limits: Limits) -> Sym {
    Sym {
        text,
        atom: Atom::Op,
        upright: true,
        scale: LARGE,
        limits,
    }
}

fn symbol(name: &str) -> Option<Sym> {
    greek(name)
        .or_else(|| more_greek(name))
        .or_else(|| relation(name))
        .or_else(|| operator(name))
        .or_else(|| function_name(name))
}

fn greek(name: &str) -> Option<Sym> {
    let text = match name {
        "alpha" => "α",
        "beta" => "β",
        "gamma" => "γ",
        "delta" => "δ",
        "epsilon" => "ϵ",
        "varepsilon" => "ε",
        "zeta" => "ζ",
        "eta" => "η",
        "theta" => "θ",
        "vartheta" => "ϑ",
        "iota" => "ι",
        "kappa" => "κ",
        "lambda" => "λ",
        "mu" => "μ",
        "nu" => "ν",
        "xi" => "ξ",
        _ => return None,
    };
    Some(sym(text, Atom::Ord, false))
}

fn more_greek(name: &str) -> Option<Sym> {
    let text = match name {
        "omicron" => "ο",
        "pi" => "π",
        "varpi" => "ϖ",
        "rho" => "ρ",
        "varrho" => "ϱ",
        "sigma" => "σ",
        "varsigma" => "ς",
        "tau" => "τ",
        "upsilon" => "υ",
        "phi" => "ϕ",
        "varphi" => "φ",
        "chi" => "χ",
        "psi" => "ψ",
        "omega" => "ω",
        "Gamma" => "Γ",
        "Delta" => "Δ",
        "Theta" => "Θ",
        "Lambda" => "Λ",
        "Xi" => "Ξ",
        "Pi" => "Π",
        "Sigma" => "Σ",
        "Upsilon" => "Υ",
        "Phi" => "Φ",
        "Psi" => "Ψ",
        "Omega" => "Ω",
        _ => return None,
    };
    // A capital Greek letter is upright, as in TeX.
    let upright = name.starts_with(|c: char| c.is_ascii_uppercase());
    Some(sym(text, Atom::Ord, upright))
}

fn relation(name: &str) -> Option<Sym> {
    let text = match name {
        "leq" | "le" => "≤",
        "geq" | "ge" => "≥",
        "neq" | "ne" => "≠",
        "approx" => "≈",
        "equiv" => "≡",
        "sim" => "∼",
        "to" | "rightarrow" => "→",
        "leftarrow" => "←",
        "Rightarrow" => "⇒",
        "Leftarrow" => "⇐",
        "in" => "∈",
        "notin" => "∉",
        "subset" => "⊂",
        "subseteq" => "⊆",
        _ => return None,
    };
    Some(sym(text, Atom::Rel, true))
}

fn operator(name: &str) -> Option<Sym> {
    Some(match name {
        "infty" | "oo" => sym("∞", Atom::Ord, true),
        "partial" => sym("∂", Atom::Ord, true),
        "nabla" => sym("∇", Atom::Ord, true),
        "ldots" => sym("…", Atom::Ord, true),
        "cdots" => sym("⋯", Atom::Ord, true),
        "cdot" => sym("·", Atom::Bin, true),
        "times" | "xx" => sym("×", Atom::Bin, true),
        "pm" => sym("±", Atom::Bin, true),
        "mp" => sym("∓", Atom::Bin, true),
        "cup" => sym("∪", Atom::Bin, true),
        "cap" => sym("∩", Atom::Bin, true),
        "forall" => sym("∀", Atom::Ord, true),
        "exists" => sym("∃", Atom::Ord, true),
        "sum" => big("∑", Limits::Above),
        "prod" => big("∏", Limits::Above),
        "int" => big("∫", Limits::Side),
        "oint" => big("∮", Limits::Side),
        _ => return None,
    })
}

fn function_name(name: &str) -> Option<Sym> {
    let text = match name {
        "sin" => "sin",
        "cos" => "cos",
        "tan" => "tan",
        "log" => "log",
        "ln" => "ln",
        "exp" => "exp",
        "lim" => "lim",
        "min" => "min",
        "max" => "max",
        _ => return None,
    };
    Some(Sym {
        limits: function_limits(name),
        ..sym(text, Atom::Op, true)
    })
}

/// A display formula puts the limits of `lim`, `min`, `max`, `det` and
/// `gcd` below them, as on a sum, and those of `log` beside it.
fn function_limits(name: &str) -> Limits {
    if matches!(name, "lim" | "min" | "max" | "det" | "gcd") {
        Limits::Above
    } else {
        Limits::Side
    }
}

#[derive(Clone, Copy)]
struct Face {
    size: f32,
    family: Family,
    /// Inside a script, which keeps only the thin space around an operator name.
    in_script: bool,
}

impl Face {
    fn script(self) -> Face {
        Face {
            size: (self.size * SCRIPT).max(SCRIPT_FLOOR),
            in_script: true,
            ..self
        }
    }

    fn at(self, size: f32) -> Face {
        Face { size, ..self }
    }
}

struct Pack {
    width: f32,
    height: f32,
    depth: f32,
    glyphs: Vec<Gly>,
    rules: Vec<Rule>,
    paths: Vec<Curve>,
}

struct Gly {
    text: String,
    x: f32,
    /// The top of the glyph, relative to the pack's baseline. Positive is down.
    y: f32,
    size: f32,
    italic: bool,
}

struct Rule {
    x1: f32,
    x2: f32,
    y: f32,
    width: f32,
}

enum Curve {
    Radical {
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        width: f32,
    },
    Paren {
        left: bool,
        x: f32,
        y0: f32,
        y1: f32,
        w: f32,
        width: f32,
    },
    Bracket {
        left: bool,
        x: f32,
        y0: f32,
        y1: f32,
        w: f32,
        width: f32,
    },
    Brace {
        left: bool,
        x: f32,
        y0: f32,
        y1: f32,
        w: f32,
        width: f32,
    },
    Bar {
        x: f32,
        y0: f32,
        y1: f32,
        double: bool,
        gap: f32,
        width: f32,
    },
}

impl Pack {
    fn empty() -> Pack {
        Pack {
            width: 0.0,
            height: 0.0,
            depth: 0.0,
            glyphs: Vec::new(),
            rules: Vec::new(),
            paths: Vec::new(),
        }
    }

    fn add(&mut self, mut child: Pack, dx: f32, dy: f32) {
        self.height = self.height.max(child.height - dy);
        self.depth = self.depth.max(child.depth + dy);
        child.shift(dx, dy);
        self.glyphs.append(&mut child.glyphs);
        self.rules.append(&mut child.rules);
        self.paths.append(&mut child.paths);
    }

    fn shift(&mut self, dx: f32, dy: f32) {
        for g in &mut self.glyphs {
            g.x += dx;
            g.y += dy;
        }
        for r in &mut self.rules {
            r.x1 += dx;
            r.x2 += dx;
            r.y += dy;
        }
        for p in &mut self.paths {
            p.shift(dx, dy);
        }
    }

    fn flush_left(mut self) -> Pack {
        let min_x = self.min_x();
        if min_x < 0.0 {
            self.shift(-min_x, 0.0);
            self.width -= min_x;
        }
        let right = self
            .paths
            .iter()
            .map(Curve::right)
            .fold(self.width, f32::max);
        let right = self
            .rules
            .iter()
            .map(|r| r.x1.max(r.x2))
            .fold(right, f32::max);
        self.width = right;
        self
    }

    fn min_x(&self) -> f32 {
        let mut m = 0.0_f32;
        for g in &self.glyphs {
            m = m.min(g.x);
        }
        for r in &self.rules {
            m = m.min(r.x1.min(r.x2));
        }
        for p in &self.paths {
            m = m.min(p.left());
        }
        m
    }
}

impl Curve {
    fn shift(&mut self, dx: f32, dy: f32) {
        match self {
            Curve::Radical {
                x0,
                y0,
                x1,
                y1,
                x2,
                y2,
                x3,
                ..
            } => {
                *x0 += dx;
                *y0 += dy;
                *x1 += dx;
                *y1 += dy;
                *x2 += dx;
                *y2 += dy;
                *x3 += dx;
            }
            Curve::Paren { x, y0, y1, .. }
            | Curve::Bracket { x, y0, y1, .. }
            | Curve::Brace { x, y0, y1, .. }
            | Curve::Bar { x, y0, y1, .. } => {
                *x += dx;
                *y0 += dy;
                *y1 += dy;
            }
        }
    }

    fn left(&self) -> f32 {
        match *self {
            Curve::Radical { x0, x1, x2, x3, .. } => x0.min(x1).min(x2).min(x3),
            Curve::Paren { x, .. }
            | Curve::Bracket { x, .. }
            | Curve::Brace { x, .. }
            | Curve::Bar { x, .. } => x,
        }
    }

    fn right(&self) -> f32 {
        match *self {
            Curve::Radical { x0, x1, x2, x3, .. } => x0.max(x1).max(x2).max(x3),
            Curve::Paren { x, w, .. } | Curve::Bracket { x, w, .. } | Curve::Brace { x, w, .. } => {
                x + w
            }
            Curve::Bar { x, double, gap, .. } => x + if double { gap } else { 0.0 },
        }
    }
}

fn layout(node: &Node, face: Face) -> Pack {
    match node {
        Node::Glyph(g) => layout_glyph(g, face).flush_left(),
        Node::Row(items) => layout_row(items, face),
        Node::Frac(a, b) => layout_frac(a, b, face).flush_left(),
        Node::Sqrt { index, body } => layout_sqrt(index.as_deref(), body, face).flush_left(),
        Node::Script {
            base,
            sup,
            sub,
            limits,
        } => layout_script(base, sup.as_deref(), sub.as_deref(), *limits, face).flush_left(),
        Node::Fence { open, body, close } => layout_fence(*open, body, *close, face).flush_left(),
        Node::Space(w) => {
            let mut pack = strut(face);
            pack.width = w * face.size;
            pack
        }
    }
}

fn layout_row(items: &[Node], face: Face) -> Pack {
    if items.is_empty() {
        return strut(face);
    }
    let mut pack = Pack::empty();
    let mut x = 0.0;
    let mut prev: Option<Atom> = None;
    for item in items {
        if let Node::Space(w) = item {
            x += w * face.size;
            continue;
        }
        let mut class = atom_class(item);
        // A minus with nothing ordinary on its left is a sign, not a sum.
        if class == Atom::Bin && prev.is_none_or(opens_unary) {
            class = Atom::Ord;
        }
        if let Some(left) = prev {
            x += gap(left, class, face);
        }
        let child = layout(item, face);
        let w = child.width;
        pack.add(child, x, 0.0);
        x += w;
        prev = Some(class);
    }
    pack.width = x.max(0.0);
    if pack.glyphs.is_empty() && pack.paths.is_empty() && pack.rules.is_empty() {
        let s = strut(face);
        pack.height = pack.height.max(s.height);
        pack.depth = pack.depth.max(s.depth);
    }
    pack.flush_left()
}

fn opens_unary(atom: Atom) -> bool {
    matches!(
        atom,
        Atom::Open | Atom::Bin | Atom::Rel | Atom::Op | Atom::Punct
    )
}

/// The space between two atoms. A script keeps only the thin space around
/// an operator name, as TeX does, so a limit reads `i=1`, not `i = 1`.
fn gap(left: Atom, right: Atom, face: Face) -> f32 {
    let around_op = (matches!(left, Atom::Op)
        && matches!(right, Atom::Ord | Atom::Open | Atom::Op))
        || (matches!(right, Atom::Op) && matches!(left, Atom::Ord | Atom::Close));
    let ems = if face.in_script {
        if around_op { THIN } else { 0.0 }
    } else if matches!(left, Atom::Rel) && matches!(right, Atom::Rel) {
        0.0
    } else if matches!(left, Atom::Rel) || matches!(right, Atom::Rel) {
        THICK
    } else if matches!(left, Atom::Bin) || matches!(right, Atom::Bin) {
        MED
    } else if around_op
        || matches!(left, Atom::Punct)
        || (matches!(left, Atom::Close) && matches!(right, Atom::Open))
    {
        THIN
    } else {
        0.0
    };
    ems * face.size
}

fn layout_glyph(g: &Glyph, face: Face) -> Pack {
    let drawn = glyph_pack(&g.text, face.at(face.size * g.scale), !g.upright);
    if g.scale <= 1.0 {
        return drawn;
    }
    // The operator's box is centered on the math axis, not on the baseline.
    let center = (drawn.height - drawn.depth) / 2.0;
    nudge(drawn, center - face.size * AXIS)
}

fn nudge(mut pack: Pack, dy: f32) -> Pack {
    pack.shift(0.0, dy);
    pack.height = (pack.height - dy).max(0.0);
    pack.depth = (pack.depth + dy).max(0.0);
    pack
}

fn layout_frac(num: &Node, den: &Node, face: Face) -> Pack {
    let num = layout(num, face);
    let den = layout(den, face);
    let em = face.size;
    let thick = (0.06 * em).max(1.2);
    let gap = 0.16 * em;
    let pad = 0.2 * em;
    let axis_y = -AXIS * em;
    let w = num.width.max(den.width) + pad * 2.0;
    let num_w = num.width;
    let den_w = den.width;
    let num_dy = axis_y - gap - thick / 2.0 - num.depth;
    let den_dy = axis_y + gap + thick / 2.0 + den.height;
    let mut pack = Pack::empty();
    pack.add(num, (w - num_w) / 2.0, num_dy);
    pack.add(den, (w - den_w) / 2.0, den_dy);
    pack.rules.push(Rule {
        x1: 0.0,
        x2: w,
        y: axis_y,
        width: thick,
    });
    pack.width = w;
    pack.height = pack.height.max(-axis_y + thick / 2.0);
    pack.depth = pack.depth.max(axis_y + thick / 2.0);
    pack
}

fn layout_sqrt(index: Option<&Node>, body: &Node, face: Face) -> Pack {
    let body = layout(body, face);
    let index = index.map(|n| layout(n, face.script()));
    let em = face.size;
    let thick = (0.06 * em).max(1.2);
    let gap = 0.14 * em;
    let pad = 0.3 * em;
    let hook = (0.58 * em).max(8.0);
    let body_x = hook;
    let bar_y = -(body.height + gap);
    let y_bot = body.depth + 0.16 * em;
    let x3 = hook + body.width + pad;
    let mut pack = Pack::empty();
    pack.add(body, body_x, 0.0);
    pack.paths.push(Curve::Radical {
        x0: hook * 0.02,
        y0: bar_y + (y_bot - bar_y) * 0.62,
        x1: hook * 0.42,
        y1: y_bot,
        x2: hook * 0.96,
        y2: bar_y,
        x3,
        width: thick.max(STROKE),
    });
    pack.width = x3;
    pack.height = pack.height.max(-bar_y + thick / 2.0);
    pack.depth = pack.depth.max(y_bot + thick / 2.0);
    if let Some(index) = index {
        let ix = hook * 0.42 - index.width;
        let iy = bar_y + (y_bot - bar_y) * 0.62 - index.depth - 0.04 * em;
        pack.add(index, ix, iy);
    }
    pack
}

fn layout_script(
    base: &Node,
    sup: Option<&Node>,
    sub: Option<&Node>,
    limits: Limits,
    face: Face,
) -> Pack {
    let base = layout(base, face);
    let script = face.script();
    let sup = sup.map(|n| layout(n, script));
    let sub = sub.map(|n| layout(n, script));
    match limits {
        Limits::Above => place_limits(base, sup, sub, face),
        Limits::Side => place_side(base, sup, sub, face),
    }
}

fn place_side(base: Pack, sup: Option<Pack>, sub: Option<Pack>, face: Face) -> Pack {
    let em = face.size;
    let mut sup_up = 0.45 * em;
    let mut sub_down = 0.22 * em;
    // A tall operator keeps its superscript at the top and its subscript at the foot.
    if base.height > em * 0.928 + 2.0
        && let Some(sup) = &sup
    {
        sup_up = sup_up.max(base.height - sup.depth - 0.12 * em);
    }
    if base.depth > em * 0.35
        && let Some(sub) = &sub
    {
        sub_down = sub_down.max(base.depth - sub.depth);
    }
    if let (Some(sup), Some(sub)) = (&sup, &sub) {
        let gap = 0.08 * em;
        let sup_bottom = -sup_up + sup.depth;
        let sub_top = sub_down - sub.height;
        if sub_top < sup_bottom + gap {
            let need = sup_bottom + gap - sub_top;
            sup_up += need / 2.0;
            sub_down += need / 2.0;
        }
    }
    let lean = if base.height > em * 1.2 {
        0.14 * em
    } else if leans(&base) {
        0.08 * em
    } else {
        0.05 * em
    };
    let x = base.width + lean;
    let script_w = sup
        .as_ref()
        .map(|s| s.width)
        .unwrap_or(0.0)
        .max(sub.as_ref().map(|s| s.width).unwrap_or(0.0));
    let mut pack = Pack::empty();
    let base_w = base.width;
    pack.add(base, 0.0, 0.0);
    if let Some(sup) = sup {
        pack.add(sup, x, -sup_up);
    }
    if let Some(sub) = sub {
        pack.add(sub, x, sub_down);
    }
    pack.width = if script_w > 0.0 { x + script_w } else { base_w };
    pack
}

fn place_limits(base: Pack, sup: Option<Pack>, sub: Option<Pack>, face: Face) -> Pack {
    let gap = 0.18 * face.size;
    let base_w = base.width;
    let base_h = base.height;
    let base_d = base.depth;
    let sup_w = sup.as_ref().map(|s| s.width).unwrap_or(0.0);
    let sub_w = sub.as_ref().map(|s| s.width).unwrap_or(0.0);
    let w = base_w.max(sup_w).max(sub_w);
    let mut pack = Pack::empty();
    pack.add(base, (w - base_w) / 2.0, 0.0);
    if let Some(sup) = sup {
        let dy = -(base_h + gap + sup.depth);
        let sw = sup.width;
        pack.add(sup, (w - sw) / 2.0, dy);
    }
    if let Some(sub) = sub {
        let dy = base_d + gap + sub.height;
        let sw = sub.width;
        pack.add(sub, (w - sw) / 2.0, dy);
    }
    pack.width = w;
    pack
}

fn leans(pack: &Pack) -> bool {
    pack.glyphs
        .iter()
        .max_by(|a, b| a.x.total_cmp(&b.x))
        .is_some_and(|g| g.italic)
}

fn layout_fence(open: Fence, body: &Node, close: Fence, face: Face) -> Pack {
    let inner = layout(body, face);
    let sample = glyph_pack("(", face, false);
    let content = inner.height + inner.depth;
    if content > (sample.height + sample.depth) * FENCE_RATIO {
        stretch_fence(open, inner, close, face)
    } else {
        glyph_fence(open, inner, close, face)
    }
}

fn glyph_fence(open: Fence, inner: Pack, close: Fence, face: Face) -> Pack {
    let mut pack = Pack::empty();
    let mut x = 0.0;
    let mut prev = None;
    if let Some(ch) = fence_char(open, false) {
        let g = glyph_pack(ch, face, false);
        x = place(&mut pack, g, x, prev, fence_atom(open, false), face);
        prev = Some(fence_atom(open, false));
    }
    let inner_w = inner.width;
    if let Some(left) = prev {
        x += gap(left, Atom::Ord, face);
    }
    pack.add(inner, x, 0.0);
    x += inner_w;
    if let Some(ch) = fence_char(close, true) {
        let class = fence_atom(close, true);
        x += gap(Atom::Ord, class, face);
        let g = glyph_pack(ch, face, false);
        let w = g.width;
        pack.add(g, x, 0.0);
        x += w;
    }
    pack.width = x;
    pack
}

fn place(
    pack: &mut Pack,
    child: Pack,
    mut x: f32,
    prev: Option<Atom>,
    class: Atom,
    face: Face,
) -> f32 {
    if let Some(left) = prev {
        x += gap(left, class, face);
    }
    let w = child.width;
    pack.add(child, x, 0.0);
    x + w
}

fn fence_atom(fence: Fence, close: bool) -> Atom {
    match fence {
        Fence::Bar | Fence::Double | Fence::None => Atom::Ord,
        _ if close => Atom::Close,
        _ => Atom::Open,
    }
}

fn fence_char(fence: Fence, close: bool) -> Option<&'static str> {
    Some(match fence {
        Fence::Paren => {
            if close {
                ")"
            } else {
                "("
            }
        }
        Fence::Bracket => {
            if close {
                "]"
            } else {
                "["
            }
        }
        Fence::Brace => {
            if close {
                "}"
            } else {
                "{"
            }
        }
        Fence::Bar => "|",
        Fence::Double => "‖",
        Fence::None => return None,
    })
}

fn stretch_fence(open: Fence, inner: Pack, close: Fence, face: Face) -> Pack {
    let em = face.size;
    let pad_x = 0.16 * em;
    let pad_y = 0.12 * em;
    let y0 = -(inner.height + pad_y);
    let y1 = inner.depth + pad_y;
    let mut pack = Pack::empty();
    let mut x = 0.0;
    if open != Fence::None {
        let w = fence_span(open, em);
        pack.paths.push(fence_curve(open, true, x, y0, y1, w));
        x += w + pad_x;
    }
    let inner_w = inner.width;
    pack.add(inner, x, 0.0);
    x += inner_w;
    if close != Fence::None {
        x += pad_x;
        let w = fence_span(close, em);
        pack.paths.push(fence_curve(close, false, x, y0, y1, w));
        x += w;
    }
    pack.width = x;
    pack.height = pack.height.max(-y0 + STROKE / 2.0);
    pack.depth = pack.depth.max(y1 + STROKE / 2.0);
    pack
}

fn fence_span(fence: Fence, em: f32) -> f32 {
    match fence {
        Fence::None => 0.0,
        Fence::Bar => 0.12 * em,
        Fence::Double => 0.28 * em,
        Fence::Brace => 0.52 * em,
        Fence::Paren | Fence::Bracket => 0.4 * em,
    }
}

fn fence_curve(fence: Fence, left: bool, x: f32, y0: f32, y1: f32, w: f32) -> Curve {
    match fence {
        Fence::Paren => Curve::Paren {
            left,
            x,
            y0,
            y1,
            w,
            width: STROKE,
        },
        Fence::Bracket => Curve::Bracket {
            left,
            x,
            y0,
            y1,
            w,
            width: STROKE,
        },
        Fence::Brace => Curve::Brace {
            left,
            x,
            y0,
            y1,
            w,
            width: STROKE,
        },
        Fence::Bar | Fence::Double | Fence::None => Curve::Bar {
            x: x + if fence == Fence::Double { 0.0 } else { w / 2.0 },
            y0,
            y1,
            double: fence == Fence::Double,
            gap: w * 0.6,
            width: STROKE,
        },
    }
}

fn glyph_pack(text: &str, face: Face, italic: bool) -> Pack {
    if text.is_empty() {
        return strut(face);
    }
    let style = run_style(face.size, italic);
    let font = Font::new(face.size, Color::BLACK);
    let block = text::styled(vec![vec![(text.to_string(), style)]], font, face.family);
    let line = &block.lines[0];
    Pack {
        width: line.width,
        height: line.ascent,
        depth: (line.height - line.ascent).max(0.0),
        glyphs: vec![Gly {
            text: text.to_string(),
            x: 0.0,
            y: -line.ascent,
            size: face.size,
            italic,
        }],
        rules: Vec::new(),
        paths: Vec::new(),
    }
}

fn strut(face: Face) -> Pack {
    let mut pack = glyph_pack("x", face, false);
    pack.width = 0.0;
    pack.glyphs.clear();
    pack
}

fn run_style(size: f32, italic: bool) -> text::Style {
    text::Style {
        size,
        bold: false,
        italic,
        underline: false,
        strike: false,
        mono: false,
        color: Color::BLACK,
    }
}

fn paint(canvas: &mut Canvas, pack: &Pack, left: f32, baseline: f32, family: Family) {
    for curve in &pack.paths {
        let (d, bounds, width) = curve.stroke(left, baseline);
        canvas.path(
            Layer::Frames,
            &d,
            bounds,
            None,
            Some(Stroke::new(Color::BLACK, width)),
        );
    }
    for rule in &pack.rules {
        canvas.line(
            Layer::Frames,
            (left + rule.x1, baseline + rule.y),
            (left + rule.x2, baseline + rule.y),
            Stroke::new(Color::BLACK, rule.width),
        );
    }
    for g in &pack.glyphs {
        let style = run_style(g.size, g.italic);
        let font = Font::new(g.size, Color::BLACK);
        let block = text::styled(vec![vec![(g.text.clone(), style)]], font, family);
        canvas.text(
            Layer::Front,
            &block,
            (left + g.x, baseline + g.y),
            Align::Left,
        );
    }
}

impl Curve {
    fn stroke(&self, left: f32, baseline: f32) -> (String, (f32, f32, f32, f32), f32) {
        let ox = left;
        let oy = baseline;
        match *self {
            Curve::Radical {
                x0,
                y0,
                x1,
                y1,
                x2,
                y2,
                x3,
                width,
            } => {
                let d = format!(
                    "M {} {} L {} {} L {} {} L {} {}",
                    svg::n(ox + x0),
                    svg::n(oy + y0),
                    svg::n(ox + x1),
                    svg::n(oy + y1),
                    svg::n(ox + x2),
                    svg::n(oy + y2),
                    svg::n(ox + x3),
                    svg::n(oy + y2),
                );
                let l = ox + x0.min(x1).min(x2).min(x3);
                let r = ox + x0.max(x1).max(x2).max(x3);
                let t = oy + y0.min(y1).min(y2);
                let b = oy + y0.max(y1).max(y2);
                (d, pad(l, t, r, b, width), width)
            }
            Curve::Paren {
                left: side,
                x,
                y0,
                y1,
                w,
                width,
            } => {
                let (d, l, r) = paren_path(side, ox + x, oy + y0, oy + y1, w);
                (d, pad(l, oy + y0, r, oy + y1, width), width)
            }
            Curve::Bracket {
                left: side,
                x,
                y0,
                y1,
                w,
                width,
            } => {
                let (d, l, r) = bracket_path(side, ox + x, oy + y0, oy + y1, w);
                (d, pad(l, oy + y0, r, oy + y1, width), width)
            }
            Curve::Brace {
                left: side,
                x,
                y0,
                y1,
                w,
                width,
            } => {
                let (d, l, r) = brace_path(side, ox + x, oy + y0, oy + y1, w);
                (d, pad(l, oy + y0, r, oy + y1, width), width)
            }
            Curve::Bar {
                x,
                y0,
                y1,
                double,
                gap,
                width,
            } => {
                let x = ox + x;
                let y0 = oy + y0;
                let y1 = oy + y1;
                let d = if double {
                    format!(
                        "M {} {} L {} {} M {} {} L {} {}",
                        svg::n(x),
                        svg::n(y0),
                        svg::n(x),
                        svg::n(y1),
                        svg::n(x + gap),
                        svg::n(y0),
                        svg::n(x + gap),
                        svg::n(y1),
                    )
                } else {
                    format!(
                        "M {} {} L {} {}",
                        svg::n(x),
                        svg::n(y0),
                        svg::n(x),
                        svg::n(y1)
                    )
                };
                let r = if double { x + gap } else { x };
                (d, pad(x, y0, r, y1, width), width)
            }
        }
    }
}

fn pad(l: f32, t: f32, r: f32, b: f32, width: f32) -> (f32, f32, f32, f32) {
    let p = width / 2.0;
    (l - p, t - p, r + p, b + p)
}

fn paren_path(left: bool, x: f32, y0: f32, y1: f32, w: f32) -> (String, f32, f32) {
    let h = y1 - y0;
    let c1 = y0 + h * 0.28;
    let c2 = y1 - h * 0.28;
    if left {
        let d = format!(
            "M {} {} C {} {} {} {} {} {}",
            svg::n(x + w),
            svg::n(y0),
            svg::n(x),
            svg::n(c1),
            svg::n(x),
            svg::n(c2),
            svg::n(x + w),
            svg::n(y1),
        );
        (d, x, x + w)
    } else {
        let d = format!(
            "M {} {} C {} {} {} {} {} {}",
            svg::n(x),
            svg::n(y0),
            svg::n(x + w),
            svg::n(c1),
            svg::n(x + w),
            svg::n(c2),
            svg::n(x),
            svg::n(y1),
        );
        (d, x, x + w)
    }
}

fn bracket_path(left: bool, x: f32, y0: f32, y1: f32, w: f32) -> (String, f32, f32) {
    let d = if left {
        format!(
            "M {} {} H {} V {} H {}",
            svg::n(x + w),
            svg::n(y0),
            svg::n(x),
            svg::n(y1),
            svg::n(x + w),
        )
    } else {
        format!(
            "M {} {} H {} V {} H {}",
            svg::n(x),
            svg::n(y0),
            svg::n(x + w),
            svg::n(y1),
            svg::n(x),
        )
    };
    (d, x, x + w)
}

fn brace_path(left: bool, x: f32, y0: f32, y1: f32, w: f32) -> (String, f32, f32) {
    let mid = (y0 + y1) / 2.0;
    let h = y1 - y0;
    let (tip, neck) = if left { (x, x + w) } else { (x + w, x) };
    let a = y0 + h * 0.18;
    let b = mid - h * 0.06;
    let c = mid + h * 0.06;
    let d = y1 - h * 0.18;
    let path = format!(
        "M {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {} C {} {} {} {} {} {}",
        svg::n(neck),
        svg::n(y0),
        svg::n(tip),
        svg::n(y0),
        svg::n(tip),
        svg::n(a),
        svg::n(tip),
        svg::n(b),
        svg::n(tip),
        svg::n(mid),
        svg::n(neck),
        svg::n(mid),
        svg::n(neck),
        svg::n(mid),
        svg::n(neck),
        svg::n(mid),
        svg::n(tip),
        svg::n(mid),
        svg::n(tip),
        svg::n(c),
        svg::n(tip),
        svg::n(y1),
        svg::n(tip),
        svg::n(d),
        svg::n(neck),
        svg::n(y1),
    );
    (path, x, x + w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn latex(body: &str) -> String {
        format!("@startlatex\n{body}\n@endlatex\n")
    }

    fn math(body: &str) -> String {
        format!("@startmath\n{body}\n@endmath\n")
    }

    fn drawn(source: &str) -> String {
        crate::render(source).unwrap()
    }

    fn root(source: &str) -> Node {
        match crate::parse(source).unwrap() {
            crate::Diagram::Formula(diagram) => diagram.root,
            other => panic!("not a formula: {other:?}"),
        }
    }

    fn frame(source: &str) -> Frame {
        match crate::parse(source).unwrap() {
            crate::Diagram::Formula(diagram) => diagram.frame,
            other => panic!("not a formula: {other:?}"),
        }
    }

    fn declined(source: &str) -> Declined {
        crate::parse(source).unwrap_err()
    }

    fn tspan<'a>(svg: &'a str, text: &str) -> &'a str {
        let needle = format!(">{text}</tspan>");
        let at = svg
            .find(&needle)
            .unwrap_or_else(|| panic!("no {text} in {svg}"));
        let start = svg[..at].rfind("<tspan").unwrap();
        &svg[start..at + needle.len()]
    }

    fn bars(svg: &str) -> Vec<f32> {
        let mut out = Vec::new();
        let mut rest = svg;
        while let Some(at) = rest.find("<line ") {
            rest = &rest[at..];
            let end = rest.find("/>").unwrap();
            let tag = &rest[..end];
            let y1 = attr(tag, "y1");
            let y2 = attr(tag, "y2");
            if (y1 - y2).abs() < 0.05 {
                out.push((attr(tag, "x2") - attr(tag, "x1")).abs());
            }
            rest = &rest[end..];
        }
        out
    }

    fn attr(tag: &str, name: &str) -> f32 {
        let key = format!("{name}=\"");
        let at = tag.find(&key).unwrap() + key.len();
        tag[at..].split('"').next().unwrap().parse().unwrap()
    }

    fn glyph_text(node: &Node) -> Option<&str> {
        match node {
            Node::Glyph(g) => Some(&g.text),
            _ => None,
        }
    }

    #[test]
    fn an_integral_keeps_tau_and_sets_the_italic() {
        let svg = drawn(&latex(r"f(t) = \int_{0}^{t} h(\tau)\,d\tau"));
        assert!(svg.contains("font-family=\"serif\""), "{svg}");
        assert!(svg.contains(">τ</tspan>"), "{svg}");
        assert!(svg.contains(">∫</tspan>"), "{svg}");
        assert!(!svg.contains("\\tau"), "{svg}");
        assert!(tspan(&svg, "f").contains("italic"), "{svg}");
        assert!(tspan(&svg, "τ").contains("italic"), "{svg}");
        assert!(!tspan(&svg, "0").contains("italic"), "{svg}");
        assert!(!svg.contains("<path"), "{svg}");
    }

    #[test]
    fn a_sum_puts_the_limits_and_the_bar_on_the_page() {
        let svg = drawn(&latex(
            "title Energy\n\\sum_{i=0}^{n} i^{2} = \\frac{(n+1)(2n+1)}{6}",
        ));
        assert!(svg.contains(">Energy</tspan>"), "{svg}");
        assert!(svg.contains(">∑</tspan>"), "{svg}");
        assert_eq!(bars(&svg).len(), 1, "{svg}");
        assert!(
            svg.contains(">#FFFFFF<") || svg.contains("#FFFFFF"),
            "{svg}"
        );
    }

    #[test]
    fn ascii_math_uses_the_same_boxes() {
        let svg = drawn(&math("sum_(i=0)^n i^2 = (n(n+1)(2n+1))/6"));
        assert!(svg.contains(">∑</tspan>"), "{svg}");
        assert_eq!(bars(&svg).len(), 1, "{svg}");
        let svg = drawn(&math(r"\tau"));
        assert!(svg.contains(">τ</tspan>"), "{svg}");
    }

    #[test]
    fn a_slash_binds_the_previous_factor() {
        let row = |source: &str| match root(source) {
            Node::Row(items) => items,
            other => panic!("{other:?}"),
        };
        let items = row(&math("a+b/c"));
        assert_eq!(glyph_text(&items[0]), Some("a"));
        assert!(matches!(
            items[1],
            Node::Glyph(Glyph {
                atom: Atom::Bin,
                ..
            })
        ));
        match &items[2] {
            Node::Frac(num, den) => {
                assert_eq!(glyph_text(num), Some("b"));
                assert_eq!(glyph_text(den), Some("c"));
            }
            other => panic!("{other:?}"),
        }
        let wide = bars(&drawn(&math("(a+b)/c")));
        let narrow = bars(&drawn(&math("a+b/c")));
        assert!(wide[0] > narrow[0] + 8.0, "{wide:?} {narrow:?}");
    }

    #[test]
    fn a_script_stops_at_one_atom() {
        let svg = drawn(&latex("x^n+1"));
        assert!(tspan(&svg, "n").contains("font-size=\"14\""), "{svg}");
        assert!(tspan(&svg, "+").contains("font-size=\"20\""), "{svg}");
        assert!(tspan(&svg, "1").contains("font-size=\"20\""), "{svg}");
    }

    #[test]
    fn a_tall_parenthesis_is_a_stroke_and_a_short_one_is_a_glyph() {
        let plain = drawn(&latex("f(t)"));
        assert!(!plain.contains("<path"), "{plain}");
        assert!(plain.contains(">(</tspan>"), "{plain}");
        let tall = drawn(&latex(r"\left( \frac{1}{2} \right)"));
        assert!(tall.contains("<path"), "{tall}");
        let grown = drawn(&latex(r"(\frac{1}{2})"));
        assert!(grown.contains("<path"), "{grown}");
    }

    #[test]
    fn a_radical_is_a_stroke() {
        let svg = drawn(&latex(r"\sqrt{x}"));
        assert!(svg.contains("<path"), "{svg}");
        assert!(svg.contains(">x</tspan>"), "{svg}");
        assert!(drawn(&latex(r"\sqrt[n]{x}")).contains("<path"));
        assert!(drawn(&math("sqrt(x+1)")).contains("<path"));
    }

    #[test]
    fn unknowns_name_the_line() {
        let err = declined(&latex("a\n\\hat{x}"));
        assert_eq!(err.line, Some(2));
        assert!(err.reason.contains("\\hat"), "{}", err.reason);
        let err = declined(&latex("x^n^2"));
        assert!(err.reason.contains("superscript"), "{}", err.reason);
        let err = declined(&latex(""));
        assert!(err.reason.contains("expression"), "{}", err.reason);
        let err = declined(&latex("<style>\n</style>\na"));
        assert_eq!(err.line, Some(1));
        assert!(err.reason.contains("<style>"), "{}", err.reason);
        let err = declined(&math("[[a,b],[c,d]]"));
        assert!(err.reason.contains("matrix"), "{}", err.reason);
        let err = declined(&latex(r"\begin{matrix} a \end{matrix}"));
        assert!(err.reason.contains("\\begin"), "{}", err.reason);
        let err = declined(&latex("a}"));
        assert!(err.reason.contains('}'), "{}", err.reason);
    }

    #[test]
    fn nesting_stops_at_one_hundred() {
        let mut body = "{".repeat(101);
        body.push('x');
        body.push_str(&"}".repeat(101));
        let err = declined(&latex(&body));
        assert!(err.reason.contains("100"), "{}", err.reason);
        let mut ok = "{".repeat(100);
        ok.push('x');
        ok.push_str(&"}".repeat(100));
        drawn(&latex(&ok));
    }

    #[test]
    fn an_upright_without_braces_counts_as_a_level() {
        let err = declined(&latex(&format!("{} x", r"\mathrm".repeat(5000))));
        assert!(err.reason.contains("100"), "{}", err.reason);
        drawn(&latex(&format!("{} x", r"\mathrm".repeat(100))));
    }

    #[test]
    fn a_slash_takes_one_item_on_either_side() {
        assert_eq!(ascii("E = 1/2 m v^2"), "[E, =, (1 / 2), m, v^2]");
        assert_eq!(ascii("ab/c"), "[a, (b / c)]");
        assert_eq!(ascii("1/xy^2"), "[(1 / x), y^2]");
        assert_eq!(ascii("-a/b"), "[−, (a / b)]");
        assert_eq!(ascii("x^2/y"), "[(x^2 / y)]");
        assert_eq!(ascii("a/b_1^2 c"), "[(a / b_1^2), c]");
        assert_eq!(ascii("a/-b c"), "[(a / [−, b]), c]");
        assert_eq!(ascii("a/-(b+c)"), "[(a / [−, ([b, +, c])])]");
        assert_eq!(ascii("sin x/2"), "[([sin, x] / 2)]");
        assert_eq!(ascii("sin^2 x/2"), "[([sin^2, x] / 2)]");
        assert_eq!(ascii("lim x/2"), "[lim, (x / 2)]");
        assert_eq!(ascii("\"sin\" x/2"), "[sin, (x / 2)]");
        assert_eq!(ascii("a/sin x y"), "[(a / [sin, x]), y]");
        assert_eq!(ascii("a/sin/2"), "[(a / sin), /, 2]");
        assert_eq!(ascii("frac a b/c"), "[((a / b) / c)]");
        assert!(declined(&math("a/+b")).reason.contains("denominator"));
        assert!(declined(&math("a/")).reason.contains("denominator"));
        assert!(declined(&math("+/b")).reason.contains("numerator"));
    }

    #[test]
    fn a_slash_drops_the_brackets_around_its_sides() {
        assert_eq!(ascii("(a+b)/c"), "[([a, +, b] / c)]");
        assert_eq!(ascii("[x]/{y}"), "[([x] / [y])]");
        assert_eq!(ascii("|x|/2"), "[(|[x]| / 2)]");
        assert_eq!(ascii("(a)^2/b"), "[(([a])^2 / b)]");
        let svg = drawn(&math("(a+b)/(c+d)"));
        assert!(!svg.contains(">(</tspan>"), "{svg}");
        assert!(!svg.contains("<path"), "{svg}");
    }

    #[test]
    fn a_slash_after_a_fraction_is_drawn() {
        assert_eq!(ascii("1/1/1/1/1"), "[(1 / 1), /, (1 / 1), /, 1]");
        let Node::Row(items) = root(&math(&format!("1{}", "/1".repeat(20000)))) else {
            panic!("not a row");
        };
        assert_eq!(items.len(), 20001);
        drawn(&math(&format!("1{}", "/1".repeat(2000))));
    }

    #[test]
    fn ascii_math_reads_the_longest_name() {
        assert_eq!(ascii("sinx"), "[sin, x]");
        assert_eq!(ascii("xin RR"), "[ξ, n, ℝ]");
        assert_eq!(ascii("x in RR"), "[x, ∈, ℝ]");
        assert_eq!(ascii("alphabeta"), "[α, β]");
        assert_eq!(ascii("omicron"), "[o, m, i, c, r, o, n]");
        assert_eq!(ascii("x <=> y +- z"), "[x, ⇔, y, ±, z]");
        assert_eq!(ascii("a ~~ b ~ c"), "[a, ≈, b, ∼, c]");
        assert_eq!(ascii("a == b <- c"), "[a, =, =, b, <, −, c]");
        assert_eq!(ascii("x |-> y // z"), "[x, ↦, y, /, z]");
        assert_eq!(ascii("AA x EE y"), "[∀, x, ∃, y]");
        assert_eq!(ascii("p and q"), "[p, [~, and, ~], q]");
        assert_eq!(ascii("{1, 2}"), "[{[1, ,, 2]}]");
    }

    #[test]
    fn ascii_math_names_it_does_not_draw_decline() {
        for (source, name) in [
            ("hat x", "`hat`"),
            ("bb A", "`bb`"),
            ("tt", "`tt`"),
            ("Lim x", "`Lim`"),
            ("floor(x)", "`floor`"),
            ("(:a, b:)", "`(:`"),
            ("{:a:}", "`{:`"),
            ("a lub b", "`lub`"),
        ] {
            let err = declined(&math(source));
            assert!(err.reason.contains(name), "{source}: {}", err.reason);
        }
        assert!(declined(&math("((a,b),(c,d))")).reason.contains("matrix"));
        assert!(declined(&math("[(1),(2)]")).reason.contains("matrix"));
        drawn(&math("((a,b),[c,d]) + f((a), b)"));
    }

    #[test]
    fn ascii_math_takes_arguments_as_asciimath_does() {
        assert_eq!(ascii("dy/dx"), "[([d, y] / [d, x])]");
        assert_eq!(ascii("f(x)/2"), "[([f, ([x])] / 2)]");
        assert_eq!(ascii("f[x]/2"), "[f, ([x] / 2)]");
        assert_eq!(ascii("a/f(x) y"), "[(a / [f, ([x])]), y]");
        assert_eq!(ascii("a/f x"), "[(a / f), x]");
        assert_eq!(ascii("sqrt x^2"), "[√{x}^2]");
        assert_eq!(ascii("sqrt(x+1)"), "[√{[x, +, 1]}]");
        assert_eq!(ascii("abs x^2 + norm(v)"), "[|x|^2, +, ‖[v]‖]");
        assert_eq!(ascii("2^-1 e^-x^2"), "[2^[−, 1], e^[−, x^2]]");
        assert_eq!(ascii("x^(n+1)_[i]"), "[x_[i]^[n, +, 1]]");
        assert_eq!(ascii("text(m/s) + mbox{a b}"), "[m/s, +, a b]");
        assert!(declined(&math("text x")).reason.contains("bracket"));
    }

    #[test]
    fn a_tex_script_takes_one_token() {
        assert_eq!(tex("x^23 a_12 x^.5"), "[x^2, 3, a_1, 2, x^., 5]");
        assert_eq!(tex("x^{23} + \\frac12"), "[x^[23], +, (1 / 2)]");
        // AsciiMath reads a number as one token.
        assert_eq!(ascii("x^23"), "[x^23]");
    }

    #[test]
    fn mathrm_is_upright_math_and_text_is_text() {
        assert_eq!(tex(r"\mathrm{m^2} \mathrm{kg\,m}"), "[[m^2], [k, g, ~, m]]");
        let svg = drawn(&latex(r"\mathrm{d}x + \mathrm{\alpha b}"));
        assert!(!tspan(&svg, "d").contains("italic"), "{svg}");
        assert!(tspan(&svg, "x").contains("italic"), "{svg}");
        assert!(tspan(&svg, "α").contains("italic"), "{svg}");
        assert!(!tspan(&svg, "b").contains("italic"), "{svg}");
        assert_eq!(tex(r"\text{a{b}c \{d\}}"), "[abc {d}]");
        assert!(declined(&latex(r"\text{m^2}")).reason.contains('^'));
    }

    #[test]
    fn greek_follows_tex() {
        assert_eq!(tex(r"\epsilon \varepsilon \phi \varphi"), "[ϵ, ε, ϕ, φ]");
        assert_eq!(ascii("epsilon varepsilon phi varphi"), "[ϵ, ε, ϕ, φ]");
        let svg = drawn(&latex(r"\Gamma \gamma"));
        assert!(!tspan(&svg, "Γ").contains("italic"), "{svg}");
        assert!(tspan(&svg, "γ").contains("italic"), "{svg}");
    }

    #[test]
    fn lim_min_and_max_put_their_limits_below() {
        let limits = |node: &Node| match node {
            Node::Script { limits, .. } => *limits,
            other => panic!("not a script: {other:?}"),
        };
        for source in [r"\lim_{x\to0}", r"\min_i", r"\max_j"] {
            let Node::Row(items) = root(&latex(source)) else {
                panic!("not a row");
            };
            assert_eq!(limits(&items[0]), Limits::Above, "{source}");
        }
        let Node::Row(items) = root(&math("lim_(x->0) det_i A log_2 x")) else {
            panic!("not a row");
        };
        let found: Vec<_> = items
            .iter()
            .filter(|n| matches!(n, Node::Script { .. }))
            .map(limits)
            .collect();
        assert_eq!(found, [Limits::Above, Limits::Above, Limits::Side]);
    }

    #[test]
    fn a_script_drops_the_space_around_a_relation() {
        let row = root(&latex("i=1"));
        let Node::Row(items) = &row else {
            panic!("not a row");
        };
        for (face, space) in [(face(), 2.0 * THICK * DISPLAY), (face().script(), 0.0)] {
            let glyphs: f32 = items.iter().map(|n| layout(n, face).width).sum();
            let width = layout(&row, face).width;
            assert!((width - glyphs - space).abs() < 0.01, "{width} {glyphs}");
        }
    }

    #[test]
    fn tex_pairs_brackets_of_either_kind_and_draws_the_rest() {
        assert_eq!(
            tex(r"[0, 1) \cup ]2, 3[ + (a"),
            "[⟦[0, ,, 1]), ∪, ], 2, ,, 3, [, +, (, a]"
        );
        assert_eq!(tex("([a)]"), "[([⟦[a])]⟧]");
        assert_eq!(tex("a/b P(A|B)"), "[a, /, b, P, ([A, |, B])]");
        assert_eq!(tex(r"\left. (a \right) b)"), "[[(, a]), b, )]");
        // Unpaired brackets cost one pass, not one per bracket.
        let Node::Row(items) = root(&latex(&"(".repeat(20000))) else {
            panic!("not a row");
        };
        assert_eq!(items.len(), 20000);
    }

    #[test]
    fn an_asciimath_bar_without_a_second_is_drawn() {
        assert_eq!(ascii("P(A|B)"), "[P, ([A, |, B])]");
        assert_eq!(ascii("{x | x > 0}"), "[{[x, |, x, >, 0]}]");
        assert_eq!(ascii("|a| b |c| d|e"), "[|[a]|, b, |[c]|, d, |, e]");
        assert_eq!(ascii("x |-> |y|"), "[x, ↦, |[y]|]");
        assert_eq!(ascii("[0, 1) uu (2, 3]"), "[⟦[0, ,, 1]), ∪, ([2, ,, 3]⟧]");
        assert!(declined(&math("(a")).reason.contains("`(` has no closing"));
    }

    #[test]
    fn a_tex_bracket_left_open_keeps_the_frame_lines() {
        for body in [
            "x = (a",
            r"\left( \frac{a}{b} \right.",
            "]2, 3[",
            r"\left\{ a \right.",
        ] {
            let frame = frame(&latex(&format!("{body}\ncaption Ratio")));
            assert!(frame.caption.is_some(), "{body}");
        }
        // A group that runs over lines keeps a title inside it.
        assert!(frame(&latex("{a\ntitle T\nb}")).title.is_none());
        assert!(
            frame(&latex("\\left( a\ntitle T\nb \\right)"))
                .title
                .is_none()
        );
        // A group left open declines instead of taking the caption in.
        assert!(declined(&latex("{a\ncaption Ratio")).reason.contains('{'));
        assert!(declined(&math("(a\ncaption Ratio")).reason.contains('('));
    }

    #[test]
    fn two_relations_have_no_space_between() {
        let row = root(&latex(":="));
        let Node::Row(items) = &row else {
            panic!("not a row");
        };
        let glyphs: f32 = items.iter().map(|n| layout(n, face()).width).sum();
        assert!((layout(&row, face()).width - glyphs).abs() < 0.01);
    }

    #[test]
    fn an_italic_keeps_its_lean_inside_the_picture() {
        let svg = drawn(&latex("f"));
        let width = attr(&svg[..svg.find('>').unwrap()], "width");
        let pack = layout(&root(&latex("f")), face());
        assert!(
            width >= pack.width + 0.08 * DISPLAY + 2.0 * MARGIN - 0.01,
            "{width} {svg}"
        );
    }

    fn face() -> Face {
        Face {
            size: DISPLAY,
            family: Canvas::new("serif", false).family(),
            in_script: false,
        }
    }

    fn tex(source: &str) -> String {
        shape(&root(&latex(source)))
    }

    fn ascii(source: &str) -> String {
        shape(&root(&math(source)))
    }

    /// A row as a list, a fraction as `(num / den)`, a fence by its
    /// brackets, a script as `base_sub^sup` and a radical as `√{body}`.
    fn shape(node: &Node) -> String {
        match node {
            Node::Glyph(g) => g.text.clone(),
            Node::Row(items) => {
                let items: Vec<_> = items.iter().map(shape).collect();
                format!("[{}]", items.join(", "))
            }
            Node::Frac(num, den) => format!("({} / {})", shape(num), shape(den)),
            Node::Script { base, sup, sub, .. } => {
                let mut out = shape(base);
                if let Some(sub) = sub {
                    out.push_str(&format!("_{}", shape(sub)));
                }
                if let Some(sup) = sup {
                    out.push_str(&format!("^{}", shape(sup)));
                }
                out
            }
            Node::Fence { open, body, close } => {
                let side = |fence: &Fence| match fence {
                    Fence::Paren => ("(", ")"),
                    Fence::Bracket => ("⟦", "⟧"),
                    Fence::Brace => ("{", "}"),
                    Fence::Bar => ("|", "|"),
                    Fence::Double => ("‖", "‖"),
                    Fence::None => ("", ""),
                };
                format!("{}{}{}", side(open).0, shape(body), side(close).1)
            }
            Node::Sqrt { body, .. } => format!("√{{{}}}", shape(body)),
            Node::Space(_) => "~".into(),
        }
    }
}
