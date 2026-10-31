//! Lines by hand ([0016](../../../docs/adr/0016-lines-by-hand.md)): the
//! bends a writer gave a line, kept as a block of comments at the end of
//! the diagram, so they travel with the file.
//!
//! ```text
//! ' lines by hand (Omaspec)
//! ' Idle -> Running via 0.5 -60
//! ' Running -> Idle #2 via 0.3 40, 0.7 40
//! ```
//!
//! A line is named by the names of its ends ([`Key`]); a bend lies relative
//! to the middles of the two ends ([`Via`]). Mermaid writes `%%` where
//! PlantUML writes `'`. The block is the mark and the comments right after
//! it; anything else ends it.

use std::fmt;

/// The block's first line, after the comment mark.
pub const MARK: &str = "lines by hand (Omaspec)";

/// No bend lies further out than this, in units or shares of the way.
const FAR: f32 = 100_000.0;

/// The most bends a line takes. The router lays the runs between them, so
/// a few do; each one more makes its search larger.
pub const MOST: usize = 8;

/// Which line: the names its ends go by (an alias where there is one), and
/// which of the lines between the same two it is, counted from 1 in the
/// order of the source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub from: String,
    pub to: String,
    pub nth: usize,
}

/// Where a bend lies: `along` the way from the middle of the box a line
/// leaves (0) to the middle of the box it enters (1), then `aside` units
/// to the right as the line travels (negative: to the left). A line from a
/// box to itself, or between two boxes with the same middle, has no way:
/// its bends lie `along` units to the right of the middle and `aside`
/// units below it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Via {
    pub along: f32,
    pub aside: f32,
}

/// A line and its bends, in the order it passes them.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub key: Key,
    pub vias: Vec<Via>,
}

/// The block in a source.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Zero-based lines of the mark and of the block's last line.
    pub first: usize,
    pub last: usize,
    /// The comment mark the block is written with, `'` or `%%`.
    pub comment: &'static str,
    /// Each line after the mark, as read.
    pub entries: Vec<Entry>,
}

/// One line of the block after the mark.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// Zero-based line in the source.
    pub line: usize,
    /// The line, or what is wrong with it.
    pub read: Result<Line, String>,
}

/// The block in `code`, if there is a mark.
pub fn read(code: &str) -> Option<Block> {
    let rows: Vec<&str> = code.split('\n').collect();
    let (first, comment) = rows.iter().enumerate().find_map(|(i, row)| {
        let row = row.trim();
        ["'", "%%"]
            .into_iter()
            .find(|c| row.strip_prefix(c).is_some_and(|rest| rest.trim() == MARK))
            .map(|c| (i, c))
    })?;
    let mut entries = Vec::new();
    for (line, row) in rows.iter().enumerate().skip(first + 1) {
        let row = row.trim();
        // `%%{init}%%` and a `'` that opens a block comment are not ours.
        let Some(text) = row
            .strip_prefix(comment)
            .filter(|t| !t.starts_with('{') && !t.starts_with('/'))
        else {
            break;
        };
        entries.push(Entry {
            line,
            read: parse(text.trim()),
        });
    }
    let last = entries.last().map_or(first, |e| e.line);
    Some(Block {
        first,
        last,
        comment,
        entries,
    })
}

/// The lines by hand in `code`: each entry that reads, the first for a key.
pub fn lines(code: &str) -> Vec<Line> {
    numbered(code).into_iter().map(|(_, line)| line).collect()
}

/// [`lines`], each with the zero-based line of its entry.
pub fn numbered(code: &str) -> Vec<(usize, Line)> {
    let mut out: Vec<(usize, Line)> = Vec::new();
    for entry in read(code).map(|b| b.entries).unwrap_or_default() {
        if let Ok(line) = entry.read
            && !out.iter().any(|(_, l)| l.key == line.key)
        {
            out.push((entry.line, line));
        }
    }
    out
}

/// The key of each line of a diagram, given the names of its ends and its
/// line in the source: lines between the same two count up in the order
/// of the source.
pub fn keys(ends: &[(String, String, usize)]) -> Vec<Key> {
    let mut order: Vec<usize> = (0..ends.len()).collect();
    order.sort_by_key(|&i| (ends[i].2, i));
    let mut keys = vec![None; ends.len()];
    let mut counts: std::collections::HashMap<(&str, &str), usize> = Default::default();
    for i in order {
        let (from, to, _) = &ends[i];
        let nth = counts.entry((from, to)).or_default();
        *nth += 1;
        keys[i] = Some(Key {
            from: from.clone(),
            to: to.clone(),
            nth: *nth,
        });
    }
    keys.into_iter().flatten().collect()
}

/// What the writer reads about an entry whose line is not there.
pub fn no_line(key: &Key) -> String {
    format!("No line runs as {key}; its bends are left out")
}

/// What the writer reads about bends that lie in a box.
pub fn left_out(key: &Key, count: usize) -> String {
    if count == 1 {
        format!("A bend of {key} lies in a box and is left out")
    } else {
        format!("{count} bends of {key} lie in boxes and are left out")
    }
}

/// What the writer reads about bends far off the drawing.
pub fn far_out(key: &Key, count: usize) -> String {
    if count == 1 {
        format!("A bend of {key} lies far off the diagram and is left out")
    } else {
        format!("{count} bends of {key} lie far off the diagram and are left out")
    }
}

/// What the writer reads about a line no way leads for through its bends.
pub fn no_way(key: &Key) -> String {
    format!("No way leads through the bends of {key}; it is drawn without them")
}

impl Via {
    /// The point of this bend for a line from a box with its middle at
    /// `from` to one with its middle at `to`.
    pub fn at(self, from: (f32, f32), to: (f32, f32)) -> (f32, f32) {
        match way(from, to) {
            Some((dx, dy, len)) => (
                from.0 + self.along * dx - self.aside * dy / len,
                from.1 + self.along * dy + self.aside * dx / len,
            ),
            None => (from.0 + self.along, from.1 + self.aside),
        }
    }

    /// The bend at `point` of a line from a box with its middle at `from`
    /// to one with its middle at `to`: what [`Via::at`] turns back into
    /// `point`.
    pub fn of(point: (f32, f32), from: (f32, f32), to: (f32, f32)) -> Via {
        let (rx, ry) = (point.0 - from.0, point.1 - from.1);
        match way(from, to) {
            Some((dx, dy, len)) => Via {
                along: (rx * dx + ry * dy) / (len * len),
                aside: (ry * dx - rx * dy) / len,
            },
            None => Via {
                along: rx,
                aside: ry,
            },
        }
    }
}

/// The way from `from` to `to` and its length, unless the two are too
/// close for a direction.
fn way(from: (f32, f32), to: (f32, f32)) -> Option<(f32, f32, f32)> {
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let len = dx.hypot(dy);
    (len >= 1.0).then_some((dx, dy, len))
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} -> {}", name(&self.from), name(&self.to))?;
        if self.nth > 1 {
            write!(f, " #{}", self.nth)?;
        }
        Ok(())
    }
}

/// An entry as written, without the comment mark: a share of the way to
/// two places, units to whole ones; both in units for a line from a box
/// to itself.
impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} via", self.key)?;
        let shares = self.key.from != self.key.to;
        for (i, via) in self.vias.iter().enumerate() {
            let along = if shares {
                number(via.along, 2)
            } else {
                number(via.along, 0)
            };
            let sep = if i == 0 { " " } else { ", " };
            write!(f, "{sep}{along} {}", number(via.aside, 0))?;
        }
        Ok(())
    }
}

/// `value` with at most `places` decimals, without trailing zeros.
fn number(value: f32, places: usize) -> String {
    let text = format!("{value:.places$}");
    let text = if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.')
    } else {
        &text
    };
    match text {
        "-0" => "0".to_owned(),
        t => t.to_owned(),
    }
}

/// A name bare where it reads back as one word, else in double quotes.
fn name(name: &str) -> String {
    let bare = !name.is_empty()
        && !name.starts_with('#')
        && name != "->"
        && name != "via"
        && !name.contains(|c: char| c.is_whitespace() || c == '"' || c == ',');
    if bare {
        name.to_owned()
    } else {
        format!("\"{name}\"")
    }
}

struct Token<'a> {
    text: &'a str,
    quoted: bool,
}

impl Token<'_> {
    fn is(&self, word: &str) -> bool {
        !self.quoted && self.text == word
    }
}

fn tokens(text: &str) -> Result<Vec<Token<'_>>, String> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        if let Some(inner) = rest.strip_prefix('"') {
            let end = inner
                .find('"')
                .ok_or("a name in quotes has no closing quote")?;
            out.push(Token {
                text: &inner[..end],
                quoted: true,
            });
            rest = &inner[end + 1..];
        } else if let Some(after) = rest.strip_prefix(',') {
            out.push(Token {
                text: ",",
                quoted: false,
            });
            rest = after;
        } else {
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ',' || c == '"')
                .unwrap_or(rest.len());
            out.push(Token {
                text: &rest[..end],
                quoted: false,
            });
            rest = &rest[end..];
        }
        rest = rest.trim_start();
    }
    Ok(out)
}

const SHAPE: &str = "a line by hand reads `From -> To via 0.5 -60`";

/// The name of one end of a line, from its token.
fn end_name(token: Option<&Token>, what: &str) -> Result<String, String> {
    match token {
        Some(t) if t.quoted || (t.text != "->" && t.text != "," && !t.is("via")) => {
            if t.text.is_empty() {
                Err(format!("the name of the {what} is empty"))
            } else {
                Ok(t.text.to_owned())
            }
        }
        _ => Err(SHAPE.to_owned()),
    }
}

/// One number of a bend, from its token.
fn value(token: Option<&Token>) -> Result<f32, String> {
    let t = token.filter(|t| !t.quoted && t.text != ",");
    let t = t.ok_or("a bend is two numbers, along the way and aside")?;
    let v: f32 = t
        .text
        .parse()
        .map_err(|_| format!("`{}` is no number", t.text))?;
    if v.is_finite() && v.abs() <= FAR {
        Ok(v)
    } else {
        Err(format!("`{}` lies too far out", t.text))
    }
}

/// One entry, without its comment mark.
fn parse(text: &str) -> Result<Line, String> {
    let tokens = tokens(text)?;
    let mut it = tokens.iter().peekable();
    let from = end_name(it.next(), "start")?;
    if !it.next().is_some_and(|t| t.is("->")) {
        return Err(SHAPE.to_owned());
    }
    let to = end_name(it.next(), "end")?;
    let mut nth = 1;
    if let Some(count) = it
        .peek()
        .filter(|t| !t.quoted)
        .and_then(|t| t.text.strip_prefix('#'))
    {
        nth = count
            .parse()
            .ok()
            .filter(|&n| n >= 1)
            .ok_or_else(|| format!("`#{count}` is no count of lines, as `#2` is"))?;
        it.next();
    }
    if !it.next().is_some_and(|t| t.is("via")) {
        return Err(SHAPE.to_owned());
    }
    let mut vias = Vec::new();
    loop {
        let along = value(it.next())?;
        let aside = value(it.next())?;
        vias.push(Via { along, aside });
        if vias.len() > MOST {
            return Err(format!("a line takes at most {MOST} bends"));
        }
        match it.next() {
            None => break,
            Some(t) if t.is(",") => {}
            Some(t) => {
                return Err(format!(
                    "`{}` follows a bend; another goes after a comma",
                    t.text
                ));
            }
        }
    }
    Ok(Line {
        key: Key { from, to, nth },
        vias,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> Line {
        parse(text).unwrap_or_else(|e| panic!("{text}: {e}"))
    }

    #[test]
    fn an_entry_reads_and_writes_back() {
        let l = line("Idle -> Running via 0.5 -60");
        assert_eq!(
            l.key,
            Key {
                from: "Idle".into(),
                to: "Running".into(),
                nth: 1
            }
        );
        assert_eq!(
            l.vias,
            [Via {
                along: 0.5,
                aside: -60.0
            }]
        );
        assert_eq!(l.to_string(), "Idle -> Running via 0.5 -60");
        for text in [
            "Running -> Idle #2 via 0.3 40, 0.7 40",
            "\"Error in Step 2\" -> Transport_S01 via 0.25 0",
            "\"via\" -> \"->\" via 1 2",
            "net.Foo -> \"#tag\" via -0.1 12",
            "Door -> Door via 90 -30",
        ] {
            assert_eq!(line(text).to_string(), text);
        }
        // Loose spacing reads, and is written tidy.
        assert_eq!(
            line("  A  ->   B  #3 via 0.333333 10.6 ,0.9 -0.4").to_string(),
            "A -> B #3 via 0.33 11, 0.9 0"
        );
    }

    #[test]
    fn a_broken_entry_says_what_is_wrong() {
        for (text, says) in [
            ("Idle Running via 0.5 -60", "reads"),
            ("Idle -> Running 0.5 -60", "reads"),
            ("Idle -> Running via 0.5", "two numbers"),
            ("Idle -> Running via 0.5 x", "no number"),
            ("Idle -> Running via 0.5 -60 0.7 1", "comma"),
            ("Idle -> Running #0 via 0.5 -60", "count"),
            ("Idle -> Running via 0.5 1e9", "too far"),
            ("Idle -> Running via NaN 1", "too far"),
            ("\"Idle -> Running via 0.5 -60", "closing quote"),
            ("\"\" -> Running via 0.5 -60", "empty"),
            ("Idle -> Running via", "two numbers"),
            (
                "Idle -> Running via 0 0, 0 0, 0 0, 0 0, 0 0, 0 0, 0 0, 0 0, 0 0",
                "at most 8",
            ),
        ] {
            let err = parse(text).expect_err(text);
            assert!(err.contains(says), "{text}: {err}");
        }
    }

    #[test]
    fn the_block_is_the_mark_and_the_comments_after_it() {
        let code = "@startuml\nA --> B\n\n' lines by hand (Omaspec)\n' A -> B via 0.5 -60\n  ' B -> A via 0.5 bad\n'/ block\n@enduml\n";
        let block = read(code).unwrap();
        assert_eq!((block.first, block.last, block.comment), (3, 5, "'"));
        assert_eq!(block.entries.len(), 2);
        assert!(block.entries[0].read.is_ok() && block.entries[1].read.is_err());
        assert_eq!(lines(code).len(), 1);

        let mermaid = "flowchart LR\n  A --> B\n  %% lines by hand (Omaspec)\n  %% A -> B via 0.5 -60\n  %% A -> B via 0.2 10\n%%{init: {}}%%\n";
        let block = read(mermaid).unwrap();
        assert_eq!((block.first, block.last, block.comment), (2, 4, "%%"));
        // The first entry for a line counts.
        assert_eq!(lines(mermaid)[0].vias[0].along, 0.5);
        assert!(read("@startuml\n' lines by hand\n@enduml\n").is_none());
    }

    #[test]
    fn lines_between_the_same_two_count_in_source_order() {
        let ends = |list: &[(&str, &str, usize)]| -> Vec<(String, String, usize)> {
            list.iter()
                .map(|&(a, b, l)| (a.to_owned(), b.to_owned(), l))
                .collect()
        };
        let keys = keys(&ends(&[
            ("A", "B", 9),
            ("B", "A", 3),
            ("A", "B", 4),
            ("A", "B", 12),
        ]));
        let shown: Vec<String> = keys.iter().map(ToString::to_string).collect();
        assert_eq!(shown, ["A -> B #2", "B -> A", "A -> B", "A -> B #3"]);
    }

    #[test]
    fn a_bend_turns_with_its_ends() {
        let close =
            |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3;
        // Down the page, right of the way is to the left on screen.
        let via = Via {
            along: 0.5,
            aside: 10.0,
        };
        assert!(close(via.at((0.0, 0.0), (0.0, 100.0)), (-10.0, 50.0)));
        // To the right, right of the way is below.
        assert!(close(via.at((0.0, 0.0), (100.0, 0.0)), (50.0, 10.0)));
        // Further apart, the same share of the way and the same units aside.
        assert!(close(via.at((0.0, 0.0), (0.0, 300.0)), (-10.0, 150.0)));
        // A line to its own box: units right and down.
        assert!(close(via.at((5.0, 5.0), (5.0, 5.0)), (5.5, 15.0)));
        for (p, from, to) in [
            ((30.0, -12.0), (0.0, 0.0), (80.0, 60.0)),
            ((7.0, 9.0), (3.0, 3.0), (3.0, 3.0)),
        ] {
            assert!(close(Via::of(p, from, to).at(from, to), p));
        }
    }
}
