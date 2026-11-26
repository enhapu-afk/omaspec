//! A marble diagram's lines, as the proposal's parser reads them
//! (`marble.tokens.ts`, `marble.parser.ts`): the header alone on its line,
//! then a statement on each line, `title …`, `axis : …`,
//! `label c "text"`, `accTitle:`, `accDescr`, a comment, or a lane,
//! `name : marbles` or `"a name" : marbles`. `title`, `axis`, `label` and
//! `marble` are words of the syntax: a lane of that name is quoted, but
//! `title : …` is a lane. A column is a character, as the reference
//! counts them, so a tab before a lane's first marble is refused where
//! it would decide a column.

use super::lane::{self, Cell};
use super::{Config, Diagram, Label, Lane, Look, Tick};
use crate::common::Text;

/// A line Mermaid refuses: zero-based, and what is said there.
type Wrong = (usize, String);

/// A lane's or the axis's string, from the character after its colon.
struct Raw<'a> {
    name: String,
    line: usize,
    byte: usize,
    /// The line's text after the colon, and the byte it starts at.
    body: &'a str,
    body_byte: usize,
    /// The character the body starts at.
    column: usize,
}

/// The marble diagram `source`, or the first line Mermaid refuses.
pub fn from_mermaid(source: &str) -> Result<Diagram, (usize, String)> {
    let lines: Vec<&str> = source
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l))
        .collect();
    let Some((header, title)) = head(&lines) else {
        return Err((0, "a marble diagram starts with `marble-beta`".into()));
    };
    let head = lines[header].trim_start_matches('\u{feff}').trim();
    let word_end = head.find(char::is_whitespace).unwrap_or(head.len());
    let word = head[..word_end].to_ascii_lowercase();
    if word != "marble-beta" && word != "marble" {
        return Err((header, "a marble diagram starts with `marble-beta`".into()));
    }
    if word_end < head.len() {
        return Err((
            header,
            "`marble-beta` stands alone on its line; what follows it is a statement of its own"
                .into(),
        ));
    }
    let mut read = Read {
        title,
        lanes: Vec::new(),
        axis: None,
        labels: Vec::new(),
    };
    // Past a line Mermaid refuses, so the first such line is said, as its
    // lanes' marbles are read once every lane's origin is known.
    let mut first: Option<Wrong> = None;
    let mut no = header + 1;
    while no < lines.len() {
        match statement(&lines, no, &mut read) {
            Ok(next) => no = next,
            Err(wrong) => {
                first.get_or_insert(wrong);
                no += 1;
            }
        }
    }
    let drawn = timeline(read);
    match (first, drawn) {
        (Some(wrong), Err(later)) if later.0 < wrong.0 => Err(later),
        (Some(wrong), _) => Err(wrong),
        (None, drawn) => drawn,
    }
}

/// What the lines have said so far.
struct Read<'a> {
    title: Option<Text>,
    lanes: Vec<Raw<'a>>,
    axis: Option<Raw<'a>>,
    labels: Vec<Label>,
}

/// The statement on line `no` into `read`; the line after it.
fn statement<'a>(lines: &[&'a str], no: usize, read: &mut Read<'a>) -> Result<usize, Wrong> {
    let line = lines[no];
    let lead = line.len() - line.trim_start().len();
    let t = line.trim();
    if t.is_empty() || t.starts_with("%%") {
        return Ok(no + 1);
    }
    let word_len = identifier(t);
    let word = &t[..word_len];
    let after = t[word_len..].trim_start();
    if word == "accTitle" && after.starts_with(':') {
        return Ok(no + 1);
    }
    if word == "accDescr" && after.starts_with(':') {
        return Ok(no + 1);
    }
    if word == "accDescr" && after.starts_with('{') {
        // To its `}`, which ends its line.
        let mut end = no;
        let mut rest = &after[1..];
        loop {
            if let Some(close) = rest.find('}') {
                if !rest[close + 1..].trim().is_empty() {
                    return Err((end, "`accDescr { … }` ends its line".into()));
                }
                break;
            }
            end += 1;
            let Some(&next) = lines.get(end) else {
                return Err((no, "`accDescr {` is never closed with `}`".into()));
            };
            rest = next;
        }
        return Ok(end + 1);
    }
    // `title` and the rest of its line; `title : …` is a lane.
    if word == "title"
        && !after.starts_with(':')
        && (word_len == t.len() || t[word_len..].starts_with(char::is_whitespace))
    {
        read.title = Some(Text {
            value: after.to_owned(),
            line: no,
            col: (!after.is_empty()).then(|| offset(line, after)),
        });
        return Ok(no + 1);
    }
    let keyword = matches!(word, "axis" | "label" | "marble" | "marble-beta");
    if word == "axis" {
        let Some(body) = after.strip_prefix(':') else {
            return Err((no, "the axis is written `axis : 0   5   10`".into()));
        };
        if read.axis.is_some() {
            return Err((no, "a second axis: a marble diagram has one".into()));
        }
        read.axis = Some(raw("axis".into(), line, no, lead, body));
        return Ok(no + 1);
    }
    if word == "label" {
        let label = label(t, lead, word_len, no, &read.labels)?;
        read.labels.push(label);
        return Ok(no + 1);
    }
    if keyword {
        return Err((
            no,
            format!("`{word}` is a word of the syntax: a lane of that name is quoted"),
        ));
    }
    // A lane: a name, or one in quotes, and its marbles.
    let (name, rest) = if let Some(quoted) = t.strip_prefix('"') {
        let Some(close) = quoted.find('"') else {
            return Err((no, "a lane's name in quotes ends with `\"`".into()));
        };
        (quoted[..close].trim().to_owned(), &quoted[close + 1..])
    } else if word_len > 0 {
        (word.to_owned(), &t[word_len..])
    } else {
        return Err((
            no,
            "a lane is written `name : marbles`: its name starts with a letter or `_`, or is in quotes".into(),
        ));
    };
    let Some(body) = rest.trim_start().strip_prefix(':') else {
        return Err((
            no,
            format!("the lane `{name}` has no marbles: they follow a colon, `{name} : -a-|`"),
        ));
    };
    if read.lanes.iter().any(|lane| lane.name == name) {
        return Err((no, format!("the lane `{name}` is written twice")));
    }
    read.lanes.push(raw(name, line, no, lead, body));
    Ok(no + 1)
}

/// What Mermaid refuses in a marble diagram, and its line: the diagram is
/// not drawn then, by Mermaid either. A text that is no marble diagram is
/// `None`.
pub fn mermaid_wrong(source: &str) -> Option<(usize, String)> {
    let lines: Vec<&str> = source.split('\n').collect();
    let (header, _) = head(&lines)?;
    let word = lines[header]
        .trim_start_matches('\u{feff}')
        .split_whitespace()
        .next()?;
    if !word.eq_ignore_ascii_case("marble-beta") && !word.eq_ignore_ascii_case("marble") {
        return None;
    }
    from_mermaid(source).err()
}

/// The line of the header among `lines`, past front matter, directives,
/// comments and blank lines, and the title the front matter gives.
fn head(lines: &[&str]) -> Option<(usize, Option<Text>)> {
    let mut title = None;
    let mut at = 0;
    loop {
        let line = *lines.get(at)?;
        let t = line.trim_start_matches('\u{feff}').trim();
        if t == "---" {
            at += 1;
            while let Some(&line) = lines.get(at) {
                if line.trim() == "---" {
                    break;
                }
                // The front matter's own title, not one of a section in it.
                if let Some(t) = line
                    .strip_prefix("title:")
                    .filter(|_| !line.starts_with([' ', '\t']))
                {
                    let value = crate::chart::unquote(t.trim());
                    title = Some(Text {
                        value: value.to_owned(),
                        line: at,
                        col: Some(offset(line, value)),
                    });
                }
                at += 1;
            }
            at += 1;
            continue;
        }
        if t.starts_with("%%{") {
            while lines.get(at).is_some_and(|l| !l.contains("}%%")) {
                at += 1;
            }
            at += 1;
            continue;
        }
        if t.is_empty() || t.starts_with("%%") {
            at += 1;
            continue;
        }
        return Some((at, title));
    }
}

/// Where `part`, a slice of `line`, starts in it.
fn offset(line: &str, part: &str) -> usize {
    (part.as_ptr() as usize).saturating_sub(line.as_ptr() as usize)
}

/// The length of the name at the start of `t`, as the reference's
/// identifier: a letter or `_`, then letters, digits, `_`, `.` and `-`.
fn identifier(t: &str) -> usize {
    let mut chars = t.char_indices();
    match chars.next() {
        Some((_, c)) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return 0,
    }
    chars
        .find(|&(_, c)| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')))
        .map_or(t.len(), |(at, _)| at)
}

/// A lane's or the axis's string on `line`, whose name starts at byte
/// `lead`; `body`, a slice of it, is the text after its colon.
fn raw<'a>(name: String, line: &'a str, no: usize, lead: usize, body: &'a str) -> Raw<'a> {
    let body_byte = offset(line, body);
    Raw {
        name,
        line: no,
        byte: lead,
        body,
        body_byte,
        column: line[..body_byte].chars().count(),
    }
}

/// `label c "text"` on line `no`, its words past the keyword at
/// `word_len` of `t`, which starts at byte `lead`.
fn label(
    t: &str,
    lead: usize,
    word_len: usize,
    no: usize,
    labels: &[Label],
) -> Result<Label, Wrong> {
    let written = "a label is written `label c \"what c means\"`";
    let rest = t[word_len..].trim_start();
    if rest.starts_with(':') {
        return Err((no, written.into()));
    }
    let name = identifier(rest);
    let symbol_len = if name > 0 {
        name
    } else {
        rest.chars()
            .next()
            .filter(|c| *c != '"' && !c.is_whitespace())
            .map_or(0, char::len_utf8)
    };
    if symbol_len == 0 {
        return Err((no, written.into()));
    }
    let symbol = &rest[..symbol_len];
    let mut chars = symbol.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return Err((
            no,
            format!("a label names one marble character, not `{symbol}`"),
        ));
    };
    let words = rest[symbol_len..].trim_start();
    let Some(quoted) = words.strip_prefix('"') else {
        return Err((no, written.into()));
    };
    let Some(close) = quoted.find('"') else {
        return Err((no, "a label's words end with `\"`".into()));
    };
    if !quoted[close + 1..].trim().is_empty() {
        return Err((no, "a label's words in quotes end its line".into()));
    }
    if labels.iter().any(|l| l.symbol == c) {
        return Err((no, format!("`{c}` is labelled twice")));
    }
    let text = &quoted[..close];
    let trimmed = text.trim();
    let start = t.len() - quoted.len() + (text.len() - text.trim_start().len());
    Ok(Label {
        symbol: c,
        text: trimmed.to_owned(),
        line: no,
        byte: lead + start,
        index: labels.len(),
    })
}

/// The lanes and the axis measured from one origin: the leftmost
/// character of any of them that is no blank.
fn timeline(read: Read) -> Result<Diagram, Wrong> {
    let Read {
        title,
        lanes,
        axis,
        labels,
    } = read;
    let origin = lanes
        .iter()
        .chain(&axis)
        .filter_map(|raw| {
            raw.body
                .chars()
                .position(|c| !c.is_whitespace())
                .map(|offset| raw.column + offset)
        })
        .min()
        .unwrap_or(0);
    // The axis's tab and each lane's marbles, the first line wrong among
    // them said.
    let mut wrong: Option<Wrong> = axis
        .as_ref()
        .filter(|axis| cells(axis, origin).iter().any(|c| c.0 == '\t'))
        .map(|axis| {
            (
                axis.line,
                "a tab in the axis: its ticks line up with spaces only".into(),
            )
        });
    let mut breaks: Vec<usize> = Vec::new();
    let mut columns = 0;
    let mut read = Vec::with_capacity(lanes.len());
    for raw in &lanes {
        let lane = match lane::read(&cells(raw, origin)) {
            Ok(lane) => lane,
            Err(said) => {
                if wrong.as_ref().is_none_or(|w| raw.line < w.0) {
                    wrong = Some((raw.line, said));
                }
                continue;
            }
        };
        columns = columns.max(lane.columns);
        breaks.extend(&lane.breaks);
        read.push(Lane {
            name: raw.name.clone(),
            line: raw.line,
            byte: raw.byte,
            items: lane.items,
            segments: lane.segments,
        });
    }
    if let Some(wrong) = wrong {
        return Err(wrong);
    }
    let mut ticks = Vec::new();
    if let Some(axis) = &axis {
        let cells = cells(axis, origin);
        columns = columns.max(cells.len());
        let mut col = 0;
        while col < cells.len() {
            if cells[col].0.is_whitespace() {
                col += 1;
                continue;
            }
            let start = col;
            while cells.get(col).is_some_and(|c| !c.0.is_whitespace()) {
                col += 1;
            }
            let text: String = cells[start..col].iter().map(|c| c.0).collect();
            if text == "/" {
                breaks.push(start);
            } else {
                ticks.push(Tick {
                    col: start,
                    text,
                    line: axis.line,
                    byte: cells[start].1.unwrap_or(axis.body_byte),
                });
            }
        }
    }
    breaks.sort_unstable();
    breaks.dedup();
    Ok(Diagram {
        title: title.filter(|t| !t.value.is_empty()),
        ticks,
        lanes: read,
        labels,
        breaks,
        columns,
        look: Look::default(),
        config: Config::default(),
    })
}

/// The characters of `raw` from `origin` on, blanks before them where it
/// starts later, without the blanks at its end.
fn cells(raw: &Raw, origin: usize) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();
    let skip = origin.saturating_sub(raw.column);
    out.extend(std::iter::repeat_n(
        (' ', None),
        raw.column.saturating_sub(origin),
    ));
    out.extend(
        raw.body
            .char_indices()
            .skip(skip)
            .map(|(at, c)| (c, Some(raw.body_byte + at))),
    );
    while out.last().is_some_and(|c| c.0.is_whitespace()) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::Item;
    use super::*;

    fn read(text: &str) -> Diagram {
        from_mermaid(text).unwrap()
    }

    fn wrong(text: &str) -> (usize, String) {
        from_mermaid(text).unwrap_err()
    }

    #[test]
    fn a_title_lanes_an_axis_and_labels() {
        let d = read(
            "marble-beta\n  title Debounced search\n  axis   : 0    5    10\n  input  : -a-b-c-----|\n  output : -------c---|\n  label a \"first key\"\n  label = \"debounce window\"\n",
        );
        assert_eq!(d.title.as_ref().unwrap().value, "Debounced search");
        assert_eq!(d.title.as_ref().unwrap().col, Some(8));
        let ticks: Vec<(usize, &str)> = d.ticks.iter().map(|t| (t.col, t.text.as_str())).collect();
        assert_eq!(ticks, [(0, "0"), (5, "5"), (10, "10")]);
        assert_eq!(d.ticks[1].byte, 16);
        let names: Vec<&str> = d.lanes.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["input", "output"]);
        assert_eq!(
            d.lanes[1].items[0],
            Item::Event {
                symbol: 'c',
                col: 7,
                stack: None,
                byte: Some(18)
            }
        );
        let labels: Vec<(char, &str, usize)> = d
            .labels
            .iter()
            .map(|l| (l.symbol, l.text.as_str(), l.index))
            .collect();
        assert_eq!(labels, [('a', "first key", 0), ('=', "debounce window", 1)]);
        assert_eq!(d.labels[0].byte, 11);
    }

    #[test]
    fn every_lane_measures_from_one_origin() {
        let d = read("marble-beta\n  source  : -a~b----\n  inner   :    [===c\n");
        assert!(matches!(d.lanes[0].items[0], Item::Event { col: 1, .. }));
        assert!(matches!(
            d.lanes[1].items[0],
            Item::Span { from: 3, to: 7, .. }
        ));
        // The axis sets the origin and the length, and its `/` breaks
        // every lane.
        let d = read("marble-beta\n axis: 0  /   20\n a   : -a-/-b\n b   : ---/--c-|\n");
        let ticks: Vec<(usize, &str)> = d.ticks.iter().map(|t| (t.col, t.text.as_str())).collect();
        assert_eq!(ticks, [(0, "0"), (7, "20")]);
        assert_eq!((d.breaks.clone(), d.columns), (vec![3], 9));
    }

    #[test]
    fn names_in_quotes_and_words_of_the_syntax() {
        // Two lanes named `title` are one written twice.
        let e =
            wrong("marble-beta\n  \"user input\" : --a--|\n  title : --a--|\n  title  : --b|\n");
        assert_eq!(e.0, 3);
        let d = read("marble-beta\n  \"user input\" : --a--|\n  title : --a--|\n");
        assert_eq!(d.lanes[0].name, "user input");
        assert_eq!(d.lanes[1].name, "title");
        assert!(d.title.is_none());
        let d = read("marble-beta\n  a : -[~~]-#\n  label ~ \"retrying\"\n  label # \"timeout\"\n");
        assert_eq!(d.labels[1].symbol, '#');
        let d = read(
            "%% leading\nmarble-beta\n  accTitle: Search\n  accDescr {\n    Keystrokes\n  }\n  %% a comment\n  input : -a-|\n",
        );
        assert_eq!(d.lanes.len(), 1);
        let d = read("---\ntitle: Shop\n---\nmarble-beta\n a: -a-|\n");
        assert_eq!(d.title.unwrap().value, "Shop");
    }

    #[test]
    fn what_mermaid_refuses_is_said_on_its_line() {
        for (text, line, said) in [
            ("marble-beta\n  input\n", 1, "no marbles"),
            ("marble-beta\n  label : --\n", 1, "label c"),
            ("marble-beta\n  label a \"oops\n", 1, "end with"),
            ("marble-beta\n  label ab \"x\"\n", 1, "one marble character"),
            ("marble-beta\n  a : --\n  a : --\n", 2, "twice"),
            (
                "marble-beta\n  label a \"x\"\n  label a \"y\"\n",
                2,
                "labelled twice",
            ),
            ("marble-beta\n  axis : 0\n  axis : 1\n", 2, "second axis"),
            ("marble-beta\n  a : -(ab-\n", 1, "never closed"),
            ("marble-beta\n  a : --)\n", 1, "no group is open"),
            ("marble-beta\n  a : -(a(b))\n", 1, "do not nest"),
            ("marble-beta\n  axis : 0\n", 9, ""),
            ("marble-beta LR\n  a : -a-\n", 0, "alone"),
            ("marble-beta\n  1st : -a-\n", 1, "starts with a letter"),
            ("marble-beta\n  axis : 0\t5\n", 1, "a tab"),
            ("marble-beta\n  marble : -a-\n", 1, "quoted"),
            ("marble-beta\n  a : -a\n  b :\t\t-b\n", 2, "a tab"),
        ] {
            if line == 9 {
                assert!(from_mermaid(text).is_ok(), "{text}");
                continue;
            }
            let (at, message) = wrong(text);
            assert_eq!(at, line, "{text}: {message}");
            assert!(message.contains(said), "{text}: {message}");
        }
        // The first line wrong is said, whichever is read first.
        assert_eq!(wrong("marble-beta\n  a : -(a\n  a : -b\n").0, 1);
        assert_eq!(wrong("marble-beta\n  axis : 0\t5\n  b : -(b\n").0, 1);
        assert_eq!(wrong("marble-beta\n  b : -(b\n  axis : 0\t5\n").0, 1);
        assert_eq!(wrong("marble-beta\n  b : -b\n  1x : -\n  c : -(c\n").0, 2);
        // A title of a section in the front matter is not the diagram's.
        let d = read("---\nconfig:\n  any:\n    title: x\n---\nmarble-beta\n a: -a-|\n");
        assert!(d.title.is_none());
        // A tab before the leftmost lane's first marble decides no column.
        assert!(from_mermaid("marble-beta\n  b :\t-b\n").is_ok());
        assert!(from_mermaid("marble-beta\n").unwrap().lanes.is_empty());
        assert_eq!(
            mermaid_wrong("marble-beta\n  a : --)\n").map(|w| w.0),
            Some(1)
        );
        assert_eq!(mermaid_wrong("flowchart LR\n  a : --)\n"), None);
    }
}
