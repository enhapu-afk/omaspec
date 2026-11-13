//! Reading a formula: the lines gathered into one expression, then TeX or
//! AsciiMath read into the tree of [`Node`]s, its brackets paired first.

use super::nodes::{
    force_upright, from_sym, glyph, is_function, is_letter_function, is_matrix, is_minus,
    is_operand, join, limits_of, numerator, plain, unbracket, unclosed,
};
use super::pairs::{bar_pairs, tex_pairs};
use super::symbols::{Word, ascii_name, function_limits, symbol};
use super::{Atom, Diagram, Fence, Glyph, MED, NEST_LIMIT, Node, Notation, THICK, THIN};
use crate::Declined;
use crate::common::Frame;
use crate::source::Line;

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

pub(super) struct Gathered {
    pub(super) text: String,
    pub(super) marks: Vec<(usize, usize)>,
    pub(super) frame: Frame,
}

/// Frame lines outside groups, and the expression with the source line of
/// each byte. A title inside a group that runs over several lines stays in
/// the expression.
pub(super) fn gather(lines: &[Line<'_>], notation: Notation) -> Result<Gathered, Declined> {
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
pub(super) fn scan(line: &str, depth: &mut i32, quote: &mut bool, notation: Notation) {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Stop {
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

pub(super) struct Cur<'a> {
    pub(super) text: &'a str,
    pub(super) at: usize,
    pub(super) marks: &'a [(usize, usize)],
    pub(super) depth: u32,
    pub(super) notation: Notation,
    /// Where the brackets (TeX) or bars (AsciiMath) start that a closing
    /// one ends, in order.
    pub(super) pairs: &'a [usize],
}

impl<'a> Cur<'a> {
    pub(super) fn rest(&self) -> &'a str {
        &self.text[self.at..]
    }

    pub(super) fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    pub(super) fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.at += c.len_utf8();
        Some(c)
    }

    pub(super) fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }

    pub(super) fn line(&self) -> usize {
        let i = self.marks.partition_point(|(end, _)| *end <= self.at);
        self.marks
            .get(i)
            .or_else(|| self.marks.last())
            .map(|(_, no)| *no)
            .unwrap_or(0)
    }

    pub(super) fn err(&self, reason: impl Into<String>) -> Declined {
        Declined::at(self.line(), reason)
    }

    pub(super) fn enter(&mut self) -> Result<(), Declined> {
        self.depth += 1;
        if self.depth > NEST_LIMIT {
            Err(self.err("nested more than 100 levels deep is not supported yet"))
        } else {
            Ok(())
        }
    }

    pub(super) fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    pub(super) fn starts_with(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    pub(super) fn eat(&mut self, s: &str) -> bool {
        if self.starts_with(s) {
            self.at += s.len();
            true
        } else {
            false
        }
    }

    pub(super) fn at_right(&self) -> bool {
        let Some(after) = self.rest().strip_prefix("\\right") else {
            return false;
        };
        after
            .chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphabetic())
    }

    pub(super) fn at_stop(&self, stop: Stop) -> bool {
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
