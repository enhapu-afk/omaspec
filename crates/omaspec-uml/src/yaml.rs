//! YAML diagrams (`@startyaml`).
//!
//! The YAML is read here into the value the JSON drawer draws
//! ([`crate::json`]): a mapping is a table of keys and values, a sequence
//! a column. `#highlight` marks a row, and a `<style>` reaches the cards
//! as `yamlDiagram { node {…} highlight {…} }`.
//!
//! Read: block mappings and sequences by their indentation, a sequence
//! under its key at the key's own depth, `- key: value`, flow sequences
//! and mappings on one line (`[a, b]`, `{a: 1}`), plain and quoted
//! scalars, `|` and `>` blocks of text, comments, and a `---` before the
//! document. A plain `null`, `~`, `true`, `false` or number is that
//! value; anything else is text.
//!
//! Anchors, aliases and tags, a `? ` key, a second document, a flow
//! value or a plain text over several lines decline and name the line.
//! The lines of a `|` block lose what they are indented by beyond the
//! block.

use crate::Declined;
use crate::common::{self, Frame};
use crate::json::{self, Diagram, Kind, NEST_LIMIT, Value};
use crate::source::Line;
use crate::style::{Element, Prop};

const BAD: &str = "the YAML cannot be read here";

/// What a `<style>` reaches in a YAML diagram: the cards and the
/// highlighted rows, under the names the JSON drawer reads.
const ELEMENTS: [Element; 2] = [
    Element::new(
        &["yamlDiagram", "node"],
        "JsonNode",
        &[Prop::BackGroundColor, Prop::LineColor, Prop::FontColor],
    ),
    Element::new(
        &["yamlDiagram", "highlight"],
        "JsonHighlight",
        &[Prop::BackGroundColor, Prop::FontColor],
    ),
];

/// One line of the YAML: where it was written, how deep it stands, and
/// its text. A `- ` entry read becomes the line of what follows the dash.
#[derive(Clone)]
struct Row {
    no: usize,
    indent: usize,
    text: String,
}

/// Parses the body of an `@startyaml` diagram.
pub fn parse(lines: &[Line<'_>]) -> Result<Diagram, Declined> {
    let mut frame = Frame::styled(&ELEMENTS);
    let mut rows = Vec::new();
    let mut paths = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if line.indent == 0 && !entry(line.text) {
            if let Some(next) = frame.line(lines, at)? {
                at = next;
                continue;
            }
            if let Some(rest) = common::keyword(line.text, "#highlight") {
                paths.push(json::highlight_path(rest, line.no)?);
                at += 1;
                continue;
            }
        }
        rows.push(Row {
            no: line.no,
            indent: line.indent,
            text: line.text.to_owned(),
        });
        at += 1;
    }
    let mut reader = Reader { rows, at: 0 };
    reader.skip();
    if reader
        .here()
        .is_some_and(|row| row.indent == 0 && row.text == "---")
    {
        reader.at += 1;
        reader.skip();
    }
    let Some(first) = reader.here() else {
        let line = lines.first().map(|l| l.no).unwrap_or(0);
        return Err(Declined::at(line, "a YAML diagram needs one value"));
    };
    let indent = first.indent;
    let mut value = reader.block(indent, 0)?;
    reader.skip();
    if let Some(row) = reader.here() {
        let why = if matches!(row.text.as_str(), "---" | "...") {
            "a second YAML document is not supported yet"
        } else {
            BAD
        };
        return Err(Declined::at(row.no, why));
    }
    for path in &paths {
        json::mark(&mut value, path);
    }
    Ok(Diagram { value, frame })
}

/// Whether a line at the first level is YAML, not a line around the
/// diagram: an item of a sequence, or a key with its colon. `title: x` is
/// a key; `title x` is the diagram's title.
fn entry(text: &str) -> bool {
    text == "-" || text.starts_with("- ") || key_end(text).is_some()
}

/// Where the colon of a `key: value` line stands: after a quoted or a
/// plain key, before a space or the end. A colon in a flow value or in
/// quotes is none.
fn key_end(text: &str) -> Option<usize> {
    if let Some(quote @ ('"' | '\'')) = text.chars().next() {
        let close = closing(text, quote)?;
        let after = &text[close + 1..];
        let colon = close + 1 + (after.len() - after.trim_start().len());
        return follows(text, colon).then_some(colon);
    }
    if text.starts_with(['[', '{', '#', '&', '*', '!', '|', '>', '@', '<']) {
        return None;
    }
    text.char_indices()
        .find(|&(index, ch)| ch == ':' && follows(text, index))
        .map(|(index, _)| index)
}

/// A colon at `at` that ends a key: a space or nothing comes after it.
fn follows(text: &str, at: usize) -> bool {
    text[at..].starts_with(':')
        && text[at + 1..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
}

/// The byte of the quote that closes the one `text` starts with. In
/// double quotes a backslash escapes, in single quotes a second quote.
fn closing(text: &str, quote: char) -> Option<usize> {
    let mut chars = text.char_indices().skip(1).peekable();
    while let Some((index, ch)) = chars.next() {
        if quote == '"' && ch == '\\' {
            chars.next();
        } else if ch == quote {
            if quote == '\'' && chars.peek().is_some_and(|&(_, next)| next == '\'') {
                chars.next();
            } else {
                return Some(index);
            }
        }
    }
    None
}

/// `text` without the comment a ` #` opens outside quotes.
fn said(text: &str) -> &str {
    let mut quote = None;
    let mut before = ' ';
    let mut chars = text.char_indices();
    while let Some((index, ch)) = chars.next() {
        match quote {
            Some('"') if ch == '\\' => {
                chars.next();
            }
            Some(open) if ch == open => quote = None,
            Some(_) => {}
            None if ch == '#' && before.is_whitespace() => return text[..index].trim_end(),
            None if matches!(ch, '"' | '\'') && !before.is_alphanumeric() => quote = Some(ch),
            None => {}
        }
        before = ch;
    }
    text.trim_end()
}

struct Reader {
    rows: Vec<Row>,
    at: usize,
}

impl Reader {
    fn here(&self) -> Option<&Row> {
        self.rows.get(self.at)
    }

    /// Past the lines that are only a comment.
    fn skip(&mut self) {
        while self.here().is_some_and(|row| row.text.starts_with('#')) {
            self.at += 1;
        }
    }

    /// The mapping or the sequence whose lines stand at `indent`.
    fn block(&mut self, indent: usize, depth: u32) -> Result<Value, Declined> {
        let Some(row) = self.here() else {
            return Ok(plain(Kind::Null));
        };
        let no = row.no;
        if depth > NEST_LIMIT {
            return Err(Declined::at(no, "the YAML is nested too deep"));
        }
        if dash(&row.text).is_some() {
            return self.sequence(indent, depth);
        }
        if key_end(&row.text).is_some() {
            return self.mapping(indent, depth);
        }
        // One value and nothing else: a scalar or a flow value.
        let text = row.text.clone();
        self.at += 1;
        self.value(&text, no, indent, depth)
    }

    fn sequence(&mut self, indent: usize, depth: u32) -> Result<Value, Declined> {
        let mut items = Vec::new();
        loop {
            self.skip();
            let Some(row) = self.here() else { break };
            if row.indent != indent {
                self.deeper(indent)?;
                break;
            }
            let Some(rest) = dash(&row.text) else { break };
            let (no, rest) = (row.no, said(rest).to_owned());
            if rest.is_empty() {
                self.at += 1;
                self.skip();
                let inner = self.here().map(|row| row.indent).filter(|&at| at > indent);
                items.push(match inner {
                    Some(inner) => self.block(inner, depth + 1)?,
                    None => plain(Kind::Null),
                });
            } else if dash(&rest).is_some() || key_end(&rest).is_some() {
                // `- key: value` and `- - item`: what follows the dash
                // is a block of its own, as deep as it stands.
                let row = &mut self.rows[self.at];
                row.indent += row.text.len() - row.text[1..].trim_start().len();
                row.text = row.text[1..].trim_start().to_owned();
                let inner = row.indent;
                items.push(self.block(inner, depth + 1)?);
            } else {
                self.at += 1;
                items.push(self.value(&rest, no, indent, depth + 1)?);
            }
        }
        Ok(plain(Kind::Array(items)))
    }

    fn mapping(&mut self, indent: usize, depth: u32) -> Result<Value, Declined> {
        let mut pairs = Vec::new();
        loop {
            self.skip();
            let Some(row) = self.here() else { break };
            if row.indent != indent {
                self.deeper(indent)?;
                break;
            }
            if dash(&row.text).is_some() {
                break;
            }
            let no = row.no;
            let Some(colon) = key_end(&row.text) else {
                let why = if row.text.starts_with("? ") {
                    "a `?` key is not supported yet"
                } else if matches!(row.text.as_str(), "---" | "...") {
                    "a second YAML document is not supported yet"
                } else {
                    BAD
                };
                return Err(Declined::at(no, why));
            };
            let key = scalar_text(row.text[..colon].trim(), no)?;
            let rest = said(row.text[colon + 1..].trim_start()).to_owned();
            self.at += 1;
            let value = if rest.is_empty() {
                self.skip();
                match self.here() {
                    Some(next) if next.indent > indent => {
                        let inner = next.indent;
                        self.block(inner, depth + 1)?
                    }
                    // A sequence may stand as deep as its key.
                    Some(next) if next.indent == indent && dash(&next.text).is_some() => {
                        self.sequence(indent, depth + 1)?
                    }
                    _ => plain(Kind::Null),
                }
            } else {
                self.value(&rest, no, indent, depth + 1)?
            };
            pairs.push((key, value));
        }
        Ok(plain(Kind::Object(pairs)))
    }

    /// A line deeper than `indent` where a block at `indent` goes on: no
    /// entry of this block, so nothing this reader can place.
    fn deeper(&self, indent: usize) -> Result<(), Declined> {
        match self.here() {
            Some(row) if row.indent > indent => Err(Declined::at(
                row.no,
                "a value over several lines is not supported yet",
            )),
            _ => Ok(()),
        }
    }

    /// The value written after a key or a dash on the line `no`, whose
    /// entry stands at `indent`: a `|` or `>` block takes the deeper
    /// lines that follow.
    fn value(
        &mut self,
        text: &str,
        no: usize,
        indent: usize,
        depth: u32,
    ) -> Result<Value, Declined> {
        let mut signs = text.chars();
        match signs.next() {
            Some(fold @ ('|' | '>')) if signs.all(|c| matches!(c, '-' | '+' | '1'..='9')) => {
                let mut lines = Vec::new();
                while let Some(row) = self.here().filter(|row| row.indent > indent) {
                    lines.push(row.text.clone());
                    self.at += 1;
                }
                let joined = lines.join(if fold == '|' { "\n" } else { " " });
                Ok(plain(Kind::String(joined)))
            }
            Some('&' | '*' | '!') => Err(Declined::at(
                no,
                "anchors, aliases and tags are not supported yet",
            )),
            Some('[' | '{') => {
                let mut flow = Flow { text, at: 0, no };
                let value = flow.value(depth)?;
                flow.blanks();
                if flow.at < text.len() {
                    return Err(Declined::at(no, BAD));
                }
                Ok(value)
            }
            _ => scalar(text, no),
        }
    }
}

/// What follows the dash of a sequence item, or `None` for another line.
fn dash(text: &str) -> Option<&str> {
    if text == "-" {
        return Some("");
    }
    text.strip_prefix("- ").map(str::trim_start)
}

fn plain(kind: Kind) -> Value {
    Value {
        kind,
        highlight: false,
        folded: false,
    }
}

/// A scalar: quoted text, or a plain word that is `null`, a truth value,
/// a number, or text.
fn scalar(text: &str, no: usize) -> Result<Value, Declined> {
    if text.starts_with(['"', '\'']) {
        return scalar_text(text, no).map(|text| plain(Kind::String(text)));
    }
    Ok(plain(match text {
        "" | "~" | "null" | "Null" | "NULL" => Kind::Null,
        "true" | "True" | "TRUE" => Kind::Bool(true),
        "false" | "False" | "FALSE" => Kind::Bool(false),
        _ if number(text) => Kind::Number(text.to_owned()),
        _ => Kind::String(text.to_owned()),
    }))
}

/// A number as YAML's core schema writes one: `12`, `-3.5`, `1e9`.
fn number(text: &str) -> bool {
    let digits = text.strip_prefix(['-', '+']).unwrap_or(text);
    digits.starts_with(|c: char| c.is_ascii_digit() || c == '.')
        && digits.chars().any(|c| c.is_ascii_digit())
        && digits
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '-' | '+'))
        && text.parse::<f64>().is_ok()
}

/// The text of a key or a scalar: what stands between its quotes, with
/// the escapes of double quotes read, or the plain text as written.
fn scalar_text(text: &str, no: usize) -> Result<String, Declined> {
    let Some(quote @ ('"' | '\'')) = text.chars().next() else {
        return Ok(text.to_owned());
    };
    let bad = || Declined::at(no, BAD);
    let close = closing(text, quote).ok_or_else(bad)?;
    if !text[close + 1..].trim().is_empty() {
        return Err(bad());
    }
    let inner = &text[1..close];
    if quote == '\'' {
        return Ok(inner.replace("''", "'"));
    }
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.next().ok_or_else(bad)? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => {}
            '0' => out.push('\0'),
            ' ' => out.push(' '),
            '/' => out.push('/'),
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'u' => {
                let hex: String = chars.by_ref().take(4).collect();
                let code = u32::from_str_radix(&hex, 16)
                    .ok()
                    .filter(|_| hex.len() == 4);
                out.push(code.and_then(char::from_u32).ok_or_else(bad)?);
            }
            _ => return Err(bad()),
        }
    }
    Ok(out)
}

/// A flow value on one line: `[a, b]`, `{a: 1, b: [2]}`.
struct Flow<'a> {
    text: &'a str,
    at: usize,
    no: usize,
}

impl Flow<'_> {
    fn bad(&self) -> Declined {
        Declined::at(self.no, BAD)
    }

    fn blanks(&mut self) {
        let rest = &self.text[self.at..];
        self.at += rest.len() - rest.trim_start().len();
    }

    fn eat(&mut self, mark: char) -> bool {
        self.blanks();
        let found = self.text[self.at..].starts_with(mark);
        if found {
            self.at += mark.len_utf8();
        }
        found
    }

    fn value(&mut self, depth: u32) -> Result<Value, Declined> {
        if depth > NEST_LIMIT {
            return Err(Declined::at(self.no, "the YAML is nested too deep"));
        }
        if self.eat('[') {
            let mut items = Vec::new();
            while !self.eat(']') {
                items.push(self.value(depth + 1)?);
                if !self.eat(',') && !self.text[self.at..].trim_start().starts_with(']') {
                    return Err(self.bad());
                }
            }
            return Ok(plain(Kind::Array(items)));
        }
        if self.eat('{') {
            let mut pairs = Vec::new();
            while !self.eat('}') {
                let key = self.word(true)?;
                let value = if self.eat(':') {
                    self.value(depth + 1)?
                } else {
                    plain(Kind::Null)
                };
                pairs.push((scalar_text(&key, self.no)?, value));
                if !self.eat(',') && !self.text[self.at..].trim_start().starts_with('}') {
                    return Err(self.bad());
                }
            }
            return Ok(plain(Kind::Object(pairs)));
        }
        let word = self.word(false)?;
        scalar(&word, self.no)
    }

    /// A quoted text with its quotes, or a plain one up to the mark that
    /// ends it: a comma, a closing bracket, and for a key its colon.
    fn word(&mut self, key: bool) -> Result<String, Declined> {
        self.blanks();
        let rest = &self.text[self.at..];
        let end = match rest.chars().next() {
            None => return Err(self.bad()),
            Some(quote @ ('"' | '\'')) => closing(rest, quote).ok_or_else(|| self.bad())? + 1,
            Some(_) => rest
                .char_indices()
                .find(|&(index, ch)| {
                    matches!(ch, ',' | ']' | '}') || (ch == ':' && (key || follows(rest, index)))
                })
                .map_or(rest.len(), |(index, _)| index),
        };
        self.at += end;
        Ok(rest[..end].trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(body: &str) -> Result<Diagram, Declined> {
        let source = format!("@startyaml\n{body}@endyaml\n");
        match crate::parse(&source)? {
            crate::Diagram::Json(diagram) => Ok(diagram),
            other => panic!("not a YAML diagram: {other:?}"),
        }
    }

    /// The value as JSON on one line, to compare with.
    fn told(value: &Value) -> String {
        match &value.kind {
            Kind::Null => "null".into(),
            Kind::Bool(b) => b.to_string(),
            Kind::Number(n) => n.clone(),
            Kind::String(s) => format!("{s:?}"),
            Kind::Array(items) => {
                let items: Vec<String> = items.iter().map(told).collect();
                format!("[{}]", items.join(","))
            }
            Kind::Object(pairs) => {
                let pairs: Vec<String> = pairs
                    .iter()
                    .map(|(key, value)| format!("{key:?}:{}", told(value)))
                    .collect();
                format!("{{{}}}", pairs.join(","))
            }
        }
    }

    fn json(body: &str) -> String {
        told(&read(body).unwrap().value)
    }

    #[test]
    fn mappings_and_sequences_follow_their_indentation() {
        let body = "name: Omaspec\nversion: 0.9\nfree: true\nhome: ~\nengines:\n  - PlantUML\n  - Mermaid\nwindow:\n  size:\n    width: 1280\n    height: 800\n  title: \"The: editor\"\n";
        assert_eq!(
            json(body),
            r#"{"name":"Omaspec","version":0.9,"free":true,"home":null,"engines":["PlantUML","Mermaid"],"window":{"size":{"width":1280,"height":800},"title":"The: editor"}}"#
        );
    }

    #[test]
    fn a_sequence_holds_mappings_and_stands_as_deep_as_its_key() {
        let body = "people:\n- name: Ada\n  born: 1815\n- name: Kim\n  likes:\n    - tea\n    - maps\n-\n  name: Lin\n- - 1\n  - 2\nlast: yes\n";
        assert_eq!(
            json(body),
            r#"{"people":[{"name":"Ada","born":1815},{"name":"Kim","likes":["tea","maps"]},{"name":"Lin"},[1,2]],"last":"yes"}"#
        );
        assert_eq!(json("- a\n- 2\n"), r#"["a",2]"#);
        assert_eq!(json("just text\n"), r#""just text""#);
    }

    #[test]
    fn flow_values_quotes_and_comments_are_read() {
        let body = "---\n# what it draws\nkinds: [state, 'a, b', \"c\\n\", 3]   # on one line\npoint: {x: 1, y: [2, 3], \"z z\": {}}\nurl: http://example.org/#top\nquote: 'it''s'\nempty: []\n\"a key\": -1.5e3\n";
        assert_eq!(
            json(body),
            r#"{"kinds":["state","a, b","c\n",3],"point":{"x":1,"y":[2,3],"z z":{}},"url":"http://example.org/#top","quote":"it's","empty":[],"a key":-1.5e3}"#
        );
    }

    #[test]
    fn blocks_of_text_are_kept_or_folded() {
        let body = "kept: |\n  one\n  two\nfolded: >-\n  one\n  two\nafter: 1\n";
        assert_eq!(
            json(body),
            r#"{"kept":"one\ntwo","folded":"one two","after":1}"#
        );
    }

    #[test]
    fn a_title_a_highlight_and_a_style_stand_around_the_yaml() {
        let body = "title The settings\n<style>\nyamlDiagram {\n  node {\n    BackGroundColor #102030\n  }\n}\n</style>\n#highlight \"window\" / \"title\"\ntitle: a key\nwindow:\n  title: Omaspec\n";
        let diagram = read(body).unwrap();
        assert_eq!(diagram.frame.title.as_ref().unwrap().value, "The settings");
        assert_eq!(
            told(&diagram.value),
            r#"{"title":"a key","window":{"title":"Omaspec"}}"#
        );
        let Kind::Object(pairs) = &diagram.value.kind else {
            panic!("a mapping");
        };
        let Kind::Object(window) = &pairs[1].1.kind else {
            panic!("a mapping");
        };
        assert!(window[0].1.highlight && !pairs[0].1.highlight);
        let svg = crate::json::draw(&diagram).unwrap();
        assert!(
            svg.contains("#102030") && svg.contains(">Omaspec<") && svg.contains("The settings"),
            "{svg}"
        );
    }

    #[test]
    fn what_it_does_not_read_declines_with_its_line() {
        for (body, line, why) in [
            ("", 0, "a YAML diagram needs one value"),
            (
                "base: &base\n  a: 1\n",
                1,
                "anchors, aliases and tags are not supported yet",
            ),
            (
                "a: 1\nb: *base\n",
                2,
                "anchors, aliases and tags are not supported yet",
            ),
            (
                "a: 1\n---\nb: 2\n",
                2,
                "a second YAML document is not supported yet",
            ),
            (
                "text: one\n  two\n",
                2,
                "a value over several lines is not supported yet",
            ),
            ("a: [1, 2\n", 1, BAD),
            ("a: \"open\n", 1, BAD),
            ("a: 1\n? b\n", 2, "a `?` key is not supported yet"),
            ("a: 1\nno colon here\n", 2, BAD),
            ("  a: 1\nb: 2\n", 2, BAD),
        ] {
            let declined = read(body).unwrap_err();
            assert_eq!(
                (declined.line, declined.reason.as_str()),
                (Some(line), why),
                "{body}"
            );
        }
        let deep = format!("{}1\n", "- ".repeat(200));
        assert_eq!(
            read(&deep).unwrap_err().reason,
            "the YAML is nested too deep"
        );
        let flow = format!("a: {}\n", "[".repeat(200));
        assert_eq!(
            read(&flow).unwrap_err().reason,
            "the YAML is nested too deep"
        );
    }
}
