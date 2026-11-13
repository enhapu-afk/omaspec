//! Lines by hand ([0016](../../../../docs/adr/0016-lines-by-hand.md)): the
//! shape a writer gave a line, kept as a block of comments at the end of
//! the diagram, so it travels with the file.
//!
//! ```text
//! ' lines by hand (Omaspec)
//! ' Idle -> Running at 0.5 -0.4
//! ' Running -> Idle #2 at 1.2 0.3, 1.2 0.8
//! ```
//!
//! A line is named by the names of its ends ([`Key`]) and runs through its
//! corners ([`Corner`]) as written, in straight runs across and down the
//! page ([`shape`]); no router lays it. Mermaid writes `%%` where PlantUML
//! writes `'`. The block is the mark and the comments right after it;
//! anything else ends it.
//!
//! The block holds boxes in place too
//! ([0021](../../../../docs/adr/0021-boxes-held-in-place.md)): `Paused after
//! Running` keeps `Paused` in the rank of `Running`, right after it across
//! the flow ([`Hold`]); `Done below Idle` ranks `Done` under `Idle`, `Error
//! level with Idle` in its rank where the layout orders it
//! ([0028](../../../../docs/adr/0028-a-box-in-another-rank.md)). And it
//! keeps lines apart
//! ([0022](../../../../docs/adr/0022-lines-bundled.md)): `apart` draws every
//! line on its own, and `apart at Name` does that for the lines of one box
//! ([`Apart`]). And it puts a label on the side of its line the writer
//! chose: `Idle -> Busy label left` ([`Label`]; `right`, `above` and
//! `below` too).

use std::fmt;

use crate::graph::Rect;

pub(crate) mod boxes;

pub use boxes::{Parted, apart, held, no_box, parted, said, unheld};

/// The block's first line, after the comment mark.
pub const MARK: &str = "lines by hand (Omaspec)";

/// No corner lies further out than this many times the span of its two
/// boxes.
const FAR: f32 = 1000.0;

/// The most corners a line takes.
pub const MOST: usize = 32;

/// Lines meet a side of a box at least this far from its corners, in
/// units.
const INSET: f32 = 10.0;

type Point = (f32, f32);

/// Which line: the names its ends go by (an alias where there is one), and
/// which of the lines between the same two it is, counted from 1 in the
/// order of the source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Key {
    pub from: String,
    pub to: String,
    pub nth: usize,
}

/// Where a corner lies: as a share of the span the two boxes of its line
/// take together ([`span`]), across (0 at its left, 1 at its right) and
/// down (0 at its top, 1 at its bottom). Each way counts on its own, so
/// corners in line stay in line when the boxes move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Corner {
    pub x: f32,
    pub y: f32,
}

/// A line and its corners, in the order it passes them.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub key: Key,
    pub corners: Vec<Corner>,
}

/// A box held against another: `name` stands where `word` puts it against
/// `beside` ([`Word`]): in its rank right after it across the flow (right
/// of it when the flow runs down, below it when it runs across) or right
/// before it, level with it, or in a rank below or above it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    pub name: String,
    pub beside: String,
    pub word: Word,
}

pub use crate::graph::hold::Word;

/// Lines that stay out of a bundle
/// ([0022](../../../../docs/adr/0022-lines-bundled.md)): every line, or the
/// lines that meet the box `at`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Apart {
    pub at: Option<String>,
}

/// The side of its line a label takes: `left` or `above` is before the
/// line across the flow, `right` or `below` after it. The word is the
/// writer's; a line running down reads `above` as `left`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Above,
    Below,
}

impl Side {
    /// Whether the label lies before its line across the flow.
    pub fn before(self) -> bool {
        matches!(self, Side::Left | Side::Above)
    }

    pub fn word(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
            Side::Above => "above",
            Side::Below => "below",
        }
    }

    fn read(word: &str) -> Option<Side> {
        match word {
            "left" => Some(Side::Left),
            "right" => Some(Side::Right),
            "above" => Some(Side::Above),
            "below" => Some(Side::Below),
            _ => None,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// A line's label on the side the writer chose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    pub key: Key,
    pub side: Side,
}

/// What an entry says: the shape of a line, where a box stands, which
/// lines stay apart, or the side a label takes.
#[derive(Debug, Clone, PartialEq)]
pub enum Hand {
    Line(Line),
    Hold(Hold),
    Apart(Apart),
    Label(Label),
}

/// A box a line leaves or enters: where it is, and whether the line may
/// meet it anywhere along a side, as a state, or only in the middle of
/// one, as a circle or a diamond.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct End {
    pub rect: Rect,
    pub flat: bool,
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
    /// What it says, or what is wrong with it.
    pub read: Result<Hand, String>,
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
        if let Ok(Hand::Line(line)) = entry.read
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
    format!("No line runs as {key}; its shape by hand is left out")
}

/// What the writer reads about corners that lie in a box at an end of
/// their line.
pub fn left_out(key: &Key, count: usize) -> String {
    if count == 1 {
        format!("A corner of {key} lies in a box at its end and is left out")
    } else {
        format!("{count} corners of {key} lie in boxes at its ends and are left out")
    }
}

/// What the writer reads about corners far off the drawing.
pub fn far_out(key: &Key, count: usize) -> String {
    if count == 1 {
        format!("A corner of {key} lies far off the diagram and is left out")
    } else {
        format!("{count} corners of {key} lie far off the diagram and are left out")
    }
}

/// What the writer reads about a line by hand that runs through a box.
pub fn through(key: &Key) -> String {
    format!("{key} runs through a box")
}

/// The span the boxes of a line take together, which its corners lie
/// relative to.
pub fn span(from: Rect, to: Rect) -> Rect {
    let (x, y) = (from.x.min(to.x), from.y.min(to.y));
    Rect {
        x,
        y,
        w: ((from.x + from.w).max(to.x + to.w) - x).max(1.0),
        h: ((from.y + from.h).max(to.y + to.h) - y).max(1.0),
    }
}

impl Corner {
    /// The point of this corner in `span`.
    pub fn at(self, span: Rect) -> Point {
        (span.x + self.x * span.w, span.y + self.y * span.h)
    }

    /// The corner at `point` in `span`: what [`Corner::at`] turns back
    /// into `point`.
    pub fn of(point: Point, span: Rect) -> Corner {
        Corner {
            x: (point.0 - span.x) / span.w,
            y: (point.1 - span.y) / span.h,
        }
    }
}

/// The line from `from` to `to` through `corners`, as it is drawn: in
/// straight runs across and down, from a side of the box it leaves to a
/// side of the one it enters. Two corners out of line get a corner between
/// them; where a box cannot meet the first or the last corner straight on,
/// the line leaves it to a corner beside it. A corner inside the box at
/// either end is left out.
pub fn shape(from: End, to: End, corners: &[Point]) -> Vec<Point> {
    let mut inner: Vec<Point> = Vec::with_capacity(corners.len() * 2);
    let outside = |p: Point| !inside(from.rect, p) && !inside(to.rect, p);
    for &p in corners.iter().filter(|&&p| outside(p)) {
        let Some(&last) = inner.last() else {
            inner.push(p);
            continue;
        };
        if same(last, p) {
            continue;
        }
        if !in_line(last, p) {
            // Turn where the line keeps on as it came.
            let came_down = inner.len() >= 2 && is_down(inner[inner.len() - 2], last);
            inner.push(if came_down {
                (last.0, p.1)
            } else {
                (p.0, last.1)
            });
        }
        inner.push(p);
    }
    let Some((&first, &last)) = inner.first().zip(inner.last()) else {
        let (a, b) = (center(from.rect), center(to.rect));
        return vec![a, b];
    };
    let goes_on = |a: Point, b: Point| (!same(a, b)).then(|| is_down(a, b));
    let leave = meet(from, first, inner.get(1).and_then(|&b| goes_on(first, b)));
    let n = inner.len();
    let mut enter = meet(
        to,
        last,
        (n >= 2).then(|| goes_on(inner[n - 2], last)).flatten(),
    );
    // A single corner the line would run to and straight back from, a
    // spike that straightening drops, stays a corner: the line leaves it
    // the other way than it came.
    if let (1, Some(&a), Some(&b)) = (n, leave.last(), enter.last())
        && spike(a, first, b)
        && let Some(down) = goes_on(a, first)
    {
        enter = meet(to, last, Some(!down));
    }
    let mut out = leave;
    out.extend(inner);
    out.extend(enter.into_iter().rev());
    straighten(out)
}

/// Where a line meets `end` on its way to or from `p`, nearest the box
/// first: a point on a side, and a corner beside the box when `p` is not
/// straight across from a side. `down` says whether the line goes on from
/// `p` down the page (or up), so that `p` stays a corner.
fn meet(end: End, p: Point, down: Option<bool>) -> Vec<Point> {
    let r = end.rect;
    let (left, right, top, bottom) = (r.x, r.x + r.w, r.y, r.y + r.h);
    let (x0, x1) = usable(left, r.w, end.flat);
    let (y0, y1) = usable(top, r.h, end.flat);
    let (beside, over) = (p.0 < left || p.0 > right, p.1 < top || p.1 > bottom);
    let side_x = if p.0 < left { left } else { right };
    let side_y = if p.1 < top { top } else { bottom };
    if over && (x0..=x1).contains(&p.0) {
        return vec![(p.0, side_y)];
    }
    if beside && (y0..=y1).contains(&p.1) {
        return vec![(side_x, p.1)];
    }
    // From a side to a corner straight across from `p`, then on to it.
    let across_first = match (beside, over) {
        (true, false) => true,
        (false, true) => false,
        // Both: arrive across the way the line does not go on.
        _ => down.unwrap_or((p.0 - r.x - r.w / 2.0).abs() >= (p.1 - r.y - r.h / 2.0).abs()),
    };
    if across_first {
        let y = p.1.clamp(y0, y1);
        vec![(side_x, y), (p.0, y)]
    } else {
        let x = p.0.clamp(x0, x1);
        vec![(x, side_y), (x, p.1)]
    }
}

/// Where along a side of `length` from `start` a line may meet it.
fn usable(start: f32, length: f32, flat: bool) -> (f32, f32) {
    let mid = start + length / 2.0;
    if flat {
        let inset = INSET.min(length / 2.0);
        (start + inset, start + length - inset)
    } else {
        (mid, mid)
    }
}

/// Whether `p` lies inside `r`, not on its border.
pub fn inside(r: Rect, p: Point) -> bool {
    p.0 > r.x && p.0 < r.x + r.w && p.1 > r.y && p.1 < r.y + r.h
}

fn center(r: Rect) -> Point {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn same(a: Point, b: Point) -> bool {
    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
}

/// Whether `a` and `b` lie straight across or straight down from each other.
fn in_line(a: Point, b: Point) -> bool {
    (a.0 - b.0).abs() < 0.01 || (a.1 - b.1).abs() < 0.01
}

/// Whether the line from `a` through `p` to `b` turns straight back at `p`.
fn spike(a: Point, p: Point, b: Point) -> bool {
    if is_down(a, p) && is_down(p, b) {
        (p.1 - a.1) * (b.1 - p.1) < 0.0
    } else if (a.1 - p.1).abs() < 0.01 && (p.1 - b.1).abs() < 0.01 {
        (p.0 - a.0) * (b.0 - p.0) < 0.0
    } else {
        false
    }
}

/// Whether the run from `a` to `b` goes down the page (or up).
fn is_down(a: Point, b: Point) -> bool {
    (a.0 - b.0).abs() < 0.01
}

/// Without repeated points and points in the middle of straight runs.
pub(crate) fn straighten(points: Vec<Point>) -> Vec<Point> {
    let across = |a: Point, b: Point| (a.1 - b.1).abs() < 0.01;
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_some_and(|&q| same(q, p)) {
            continue;
        }
        if let [.., a, b] = out[..]
            && ((is_down(a, b) && is_down(b, p)) || (across(a, b) && across(b, p)))
        {
            out.pop();
        }
        out.push(p);
    }
    out
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

/// An entry as written, without the comment mark.
impl fmt::Display for Hold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} {}",
            name(&self.name),
            self.word,
            name(&self.beside)
        )
    }
}

/// An entry as written, without the comment mark.
impl fmt::Display for Apart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.at {
            None => write!(f, "apart"),
            Some(at) => write!(f, "apart at {}", name(at)),
        }
    }
}

impl fmt::Display for Hand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hand::Line(line) => line.fmt(f),
            Hand::Hold(hold) => hold.fmt(f),
            Hand::Apart(apart) => apart.fmt(f),
            Hand::Label(label) => label.fmt(f),
        }
    }
}

/// An entry as written, without the comment mark.
impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} label {}", self.key, self.side.word())
    }
}

/// An entry as written, without the comment mark: each corner as two
/// shares, to a thousandth.
impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at", self.key)?;
        for (i, corner) in self.corners.iter().enumerate() {
            let sep = if i == 0 { " " } else { ", " };
            write!(f, "{sep}{} {}", number(corner.x, 3), number(corner.y, 3))?;
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
        && name != "at"
        && !HOLD_WORDS.contains(&name)
        && name != "apart"
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

const SHAPE: &str = "a line by hand reads `From -> To at 0.5 -0.4`";
const HELD: &str = "a box held in place reads `Box after Neighbor`, `before`, `below`, \
                    `above`, `right of`, `left of` or `level with`";
/// The words that start holding a box; a box of that name is written in
/// double quotes.
const HOLD_WORDS: [&str; 7] = [
    "after", "before", "below", "above", "right", "left", "level",
];
const APART: &str = "lines stay apart as `apart`, or as `apart at Name`";
const LABEL: &str = "a label's side reads `From -> To label left` (or right, above, below)";

/// The name of one end of a line, from its token.
fn end_name(token: Option<&Token>, what: &str) -> Result<String, String> {
    match token {
        Some(t) if t.quoted || (t.text != "->" && t.text != "," && !t.is("at")) => {
            if t.text.is_empty() {
                Err(format!("the name of the {what} is empty"))
            } else {
                Ok(t.text.to_owned())
            }
        }
        _ => Err(SHAPE.to_owned()),
    }
}

/// One number of a corner, from its token.
fn value(token: Option<&Token>) -> Result<f32, String> {
    let t = token.filter(|t| !t.quoted && t.text != ",");
    let t = t.ok_or("a corner is two numbers, across and down")?;
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

/// One entry, without its comment mark: a line by hand, a box held in
/// place, or lines kept apart.
fn parse(text: &str) -> Result<Hand, String> {
    let tokens = tokens(text)?;
    if tokens.first().is_some_and(|t| t.is("apart"))
        && (tokens.len() == 1 || tokens.get(1).is_some_and(|t| t.is("at")))
    {
        return kept_apart(&tokens).map(Hand::Apart);
    }
    match tokens.get(1) {
        Some(t) if HOLD_WORDS.iter().any(|word| t.is(word)) => hold(&tokens).map(Hand::Hold),
        Some(t) if t.is("under") || t.is("over") => Err(format!(
            "`{}` reads `{}`: {HELD}",
            t.text,
            if t.is("under") { "below" } else { "above" }
        )),
        _ => line(&tokens),
    }
}

/// The labels by hand in `code`: each entry that reads, the first for a
/// key.
pub fn labels(code: &str) -> Vec<Label> {
    let mut out: Vec<Label> = Vec::new();
    for entry in read(code).map(|b| b.entries).unwrap_or_default() {
        if let Ok(Hand::Label(label)) = entry.read
            && !out.iter().any(|l| l.key == label.key)
        {
            out.push(label);
        }
    }
    out
}

/// Lines kept apart: `apart`, or `apart at Name`.
fn kept_apart(tokens: &[Token]) -> Result<Apart, String> {
    match tokens {
        [one] if one.is("apart") => Ok(Apart { at: None }),
        [one, at, name] if one.is("apart") && at.is("at") && (name.quoted || !name.is("at")) => {
            if name.text.is_empty() {
                Err("the name of the box is empty".to_owned())
            } else {
                Ok(Apart {
                    at: Some(name.text.to_owned()),
                })
            }
        }
        _ => Err(APART.to_owned()),
    }
}

/// A box held in place: `Box after Neighbor`, `before`, `below`, `above`,
/// `right of`, `left of` or `level with` it.
fn hold(tokens: &[Token]) -> Result<Hold, String> {
    // The word stands second, so a box or a neighbor named like one
    // (`left after Running`) still reads; the writer quotes it all the same.
    let named = |t: Option<&Token>, what: &str| match t {
        Some(t) if t.quoted || !t.is(",") => {
            if t.text.is_empty() {
                Err(format!("the name of the {what} is empty"))
            } else {
                Ok(t.text.to_owned())
            }
        }
        _ => Err(HELD.to_owned()),
    };
    // The word, one or two tokens long, and where the neighbor stands.
    let (word, beside) = Word::ALL
        .iter()
        .find_map(|word| {
            let mut parts = word.text().split(' ');
            let first = parts.next()?;
            let second = parts.next();
            if !tokens.get(1).is_some_and(|t| t.is(first)) {
                return None;
            }
            match second {
                None => Some((*word, 2)),
                Some(second) => tokens
                    .get(2)
                    .is_some_and(|t| t.is(second))
                    .then_some((*word, 3)),
            }
        })
        .ok_or_else(|| HELD.to_owned())?;
    if tokens.len() != beside + 1 {
        return Err(HELD.to_owned());
    }
    Ok(Hold {
        name: named(tokens.first(), "box")?,
        word,
        beside: named(tokens.get(beside), "neighbor")?,
    })
}

/// A line by hand, from its tokens.
fn line(tokens: &[Token]) -> Result<Hand, String> {
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
    // `label left`: the side of the line its label takes.
    if it.peek().is_some_and(|t| t.is("label")) {
        it.next();
        let side = it
            .next()
            .filter(|t| !t.quoted)
            .and_then(|t| Side::read(t.text))
            .ok_or_else(|| LABEL.to_owned())?;
        if it.next().is_some() {
            return Err(LABEL.to_owned());
        }
        return Ok(Hand::Label(Label {
            key: Key { from, to, nth },
            side,
        }));
    }
    if !it.next().is_some_and(|t| t.is("at")) {
        return Err(SHAPE.to_owned());
    }
    let mut corners = Vec::new();
    loop {
        let x = value(it.next())?;
        let y = value(it.next())?;
        corners.push(Corner { x, y });
        if corners.len() > MOST {
            return Err(format!("a line takes at most {MOST} corners"));
        }
        match it.next() {
            None => break,
            Some(t) if t.is(",") => {}
            Some(t) => {
                return Err(format!(
                    "`{}` follows a corner; another goes after a comma",
                    t.text
                ));
            }
        }
    }
    Ok(Hand::Line(Line {
        key: Key { from, to, nth },
        corners,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str) -> Line {
        match parse(text) {
            Ok(Hand::Line(line)) => line,
            other => panic!("{text}: {other:?}"),
        }
    }

    #[test]
    fn a_box_is_held_against_a_neighbor() {
        let held = |text: &str| match parse(text) {
            Ok(Hand::Hold(hold)) => hold,
            other => panic!("{text}: {other:?}"),
        };
        let hold = held("Paused after Running");
        assert_eq!(
            hold,
            Hold {
                name: "Paused".to_owned(),
                beside: "Running".to_owned(),
                word: Word::After,
            }
        );
        assert_eq!(hold.to_string(), "Paused after Running");
        let quoted = held(r#""Wait here" before "after""#);
        assert!(quoted.word == Word::Before && quoted.beside == "after");
        assert_eq!(quoted.to_string(), r#""Wait here" before "after""#);
        // Into another rank, or level with it: every word reads back.
        for word in Word::ALL {
            let text = format!("Done {word} Idle");
            assert_eq!(held(&text).word, word, "{text}");
            assert_eq!(held(&text).to_string(), text);
        }
        assert_eq!(
            held(r#""right" right of left"#).to_string(),
            r#""right" right of "left""#
        );
        for wrong in [
            "Paused after",
            "Paused after Running too",
            "Done right Idle",
            "Done level Idle",
            "Done left of",
            "Done right of Idle too",
        ] {
            assert!(parse(wrong).is_err(), "{wrong}");
        }
        assert!(parse("Done under Idle").unwrap_err().contains("`below`"));
        // A box named like a word, written bare as entries were before.
        let left = held("left after Running");
        assert_eq!((left.name.as_str(), left.word), ("left", Word::After));
        assert_eq!(left.to_string(), r#""left" after Running"#);
        let code = "' lines by hand (Omaspec)\n' A after B\n' A before C\n' B -> C at 0.5 0.5\n";
        let kept = super::held(code);
        assert_eq!(kept.len(), 1, "the first entry for a box holds");
        assert_eq!((kept[0].0, kept[0].1.beside.as_str()), (1, "B"));
        assert_eq!(numbered(code).len(), 1);
    }

    #[test]
    fn lines_stay_apart_for_the_diagram_or_for_one_box() {
        let apart = |text: &str| match parse(text) {
            Ok(Hand::Apart(apart)) => apart,
            other => panic!("{text}: {other:?}"),
        };
        assert_eq!(apart("apart"), Apart { at: None });
        assert_eq!(apart("apart").to_string(), "apart");
        let one = apart("apart at Error");
        assert_eq!(one.at.as_deref(), Some("Error"));
        assert_eq!(one.to_string(), "apart at Error");
        let quoted = apart(r#"apart at "Wait here""#);
        assert_eq!(quoted.to_string(), r#"apart at "Wait here""#);
        // A box named apart is still held in place.
        assert!(matches!(parse("apart after Running"), Ok(Hand::Hold(_))));
        for wrong in ["apart at", "apart at Error too", "apart Error"] {
            assert!(parse(wrong).is_err(), "{wrong}");
        }
        let code =
            "' lines by hand (Omaspec)\n' apart\n' apart\n' apart at Idle\n' apart at Idle\n";
        let kept = super::apart(code);
        assert_eq!(kept.len(), 2, "the first of each stays");
        assert!(kept[0].1.at.is_none());
        assert_eq!(kept[1].1.at.as_deref(), Some("Idle"));
    }

    #[test]
    fn an_entry_reads_and_writes_back() {
        let l = line("Idle -> Running at 0.5 -0.4");
        assert_eq!(
            l.key,
            Key {
                from: "Idle".into(),
                to: "Running".into(),
                nth: 1
            }
        );
        assert_eq!(l.corners, [Corner { x: 0.5, y: -0.4 }]);
        assert_eq!(l.to_string(), "Idle -> Running at 0.5 -0.4");
        for text in [
            "Running -> Idle #2 at 1.2 0.3, 1.2 0.8",
            "\"Error in Step 2\" -> Transport_S01 at 0.25 0",
            "\"at\" -> \"->\" at 1 2",
            "net.Foo -> \"#tag\" at -0.1 12",
            "Door -> Door at 1.5 0.5, 1.5 1.5, 0.5 1.5",
        ] {
            assert_eq!(line(text).to_string(), text);
        }
        // Loose spacing reads, and is written tidy.
        assert_eq!(
            line("  A  ->   B  #3 at 0.333333 10.6 ,0.9 -0.0004").to_string(),
            "A -> B #3 at 0.333 10.6, 0.9 0"
        );
    }

    #[test]
    fn a_broken_entry_says_what_is_wrong() {
        let many = vec!["0 0"; MOST + 1].join(", ");
        for (text, says) in [
            ("Idle Running at 0.5 -0.4", "reads"),
            ("Idle -> Running 0.5 -0.4", "reads"),
            ("Idle -> Running via 0.5 -60", "reads"),
            ("Idle -> Running at 0.5", "two numbers"),
            ("Idle -> Running at 0.5 x", "no number"),
            ("Idle -> Running at 0.5 -0.4 0.7 1", "comma"),
            ("Idle -> Running #0 at 0.5 -0.4", "count"),
            ("Idle -> Running at 0.5 1e9", "too far"),
            ("Idle -> Running at NaN 1", "too far"),
            ("\"Idle -> Running at 0.5 -0.4", "closing quote"),
            ("\"\" -> Running at 0.5 -0.4", "empty"),
            ("Idle -> Running at", "two numbers"),
            (&format!("Idle -> Running at {many}"), "at most 32"),
        ] {
            let err = parse(text).expect_err(text);
            assert!(err.contains(says), "{text}: {err}");
        }
    }

    #[test]
    fn the_block_is_the_mark_and_the_comments_after_it() {
        let code = "@startuml\nA --> B\n\n' lines by hand (Omaspec)\n' A -> B at 0.5 -0.4\n  ' B -> A at 0.5 bad\n'/ block\n@enduml\n";
        let block = read(code).unwrap();
        assert_eq!((block.first, block.last, block.comment), (3, 5, "'"));
        assert_eq!(block.entries.len(), 2);
        assert!(block.entries[0].read.is_ok() && block.entries[1].read.is_err());
        assert_eq!(lines(code).len(), 1);

        let mermaid = "flowchart LR\n  A --> B\n  %% lines by hand (Omaspec)\n  %% A -> B at 0.5 -0.4\n  %% A -> B at 0.2 1\n%%{init: {}}%%\n";
        let block = read(mermaid).unwrap();
        assert_eq!((block.first, block.last, block.comment), (2, 4, "%%"));
        // The first entry for a line counts.
        assert_eq!(lines(mermaid)[0].corners[0].x, 0.5);
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

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    fn state(x: f32, y: f32) -> End {
        End {
            rect: rect(x, y, 80.0, 40.0),
            flat: true,
        }
    }

    #[test]
    fn a_corner_keeps_its_share_of_the_span_each_way() {
        let (a, b) = (rect(0.0, 0.0, 80.0, 40.0), rect(200.0, 160.0, 80.0, 40.0));
        let span = span(a, b);
        assert_eq!(span, rect(0.0, 0.0, 280.0, 200.0));
        let corner = Corner { x: 0.5, y: -0.25 };
        assert_eq!(corner.at(span), (140.0, -50.0));
        assert_eq!(Corner::of((140.0, -50.0), span), corner);
        // Two corners in line stay in line when the boxes move apart.
        let wider = super::span(a, rect(400.0, 300.0, 80.0, 40.0));
        let (p, q) = (
            Corner { x: 0.5, y: 0.1 }.at(wider),
            Corner { x: 0.5, y: 0.9 }.at(wider),
        );
        assert_eq!(p.0, q.0);
    }

    #[test]
    fn a_shape_runs_straight_through_its_corners_from_side_to_side() {
        // Ready on the left, Refill lower right; the line out to the right,
        // down, and in from the left.
        let (from, to) = (state(0.0, 0.0), state(300.0, 200.0));
        let corners = [(150.0, 20.0), (150.0, 220.0)];
        assert_eq!(
            shape(from, to, &corners),
            [(80.0, 20.0), (150.0, 20.0), (150.0, 220.0), (300.0, 220.0)]
        );
        // One corner, not straight across from either box: the line leaves
        // and enters beside it.
        assert_eq!(
            shape(from, to, &[(150.0, 120.0)]),
            [(80.0, 30.0), (150.0, 30.0), (150.0, 210.0), (300.0, 210.0)]
        );
        // One corner below both boxes, which lie side by side: the line
        // turns there instead of running down to it and back.
        let (left, right) = (state(0.0, 0.0), state(200.0, 0.0));
        let out = shape(right, left, &[(140.0, 60.0)]);
        assert!(out.contains(&(140.0, 60.0)), "{out:?}");
        // Two corners out of line get a corner between them.
        let out = shape(from, to, &[(150.0, 20.0), (250.0, 220.0)]);
        assert!(
            out.windows(2).all(|w| w[0].0 == w[1].0 || w[0].1 == w[1].1),
            "{out:?}"
        );
        // A circle is met in the middle of a side.
        let start = End {
            rect: rect(0.0, 0.0, 20.0, 20.0),
            flat: false,
        };
        assert_eq!(
            shape(start, to, &[(10.0, 220.0)]),
            [(10.0, 20.0), (10.0, 220.0), (300.0, 220.0)]
        );
        // A line to its own box goes out and comes back.
        let own = shape(from, from, &[(120.0, 20.0), (120.0, 80.0), (40.0, 80.0)]);
        assert_eq!(
            own,
            [
                (80.0, 20.0),
                (120.0, 20.0),
                (120.0, 80.0),
                (40.0, 80.0),
                (40.0, 40.0)
            ]
        );
    }
}
