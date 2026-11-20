//! What a Mermaid diagram's first line says: the kind it names, as mmdr
//! reads it and as mermaid.js writes it, or what mmdr makes of a line that
//! names none. Render and lint judge the line alike, so a diagram lint
//! passes is drawn and one render refuses is an error to lint (REQ-089).
//!
//! mmdr (mermaid-rs-renderer 0.3.1, MIT) reads the first line without a
//! comment at its end, takes a kind's word lowercased and ending where no
//! letter or digit follows, and else reads any line with a `-` or `=`
//! outside brackets and quotes as a flowchart's. [`read`], [`masked`] and
//! the arrow are its `strip_trailing_comment`, `mask_bracket_content` and
//! `ARROW_TOKEN_RE`, written again here.

/// Each kind of Mermaid diagram: the word mmdr reads, lowercased, and the
/// ways mermaid.js takes it, the first as it is usually written (a kind
/// still `-beta` there taken without too, as newer mermaid.js versions
/// take some: a hint said wrongly is worse than one left out). Every kind
/// with a drawer of Omaspec's own is among them (`NATIVE` in omaspec-render,
/// held to it by a test there).
const KINDS: &[(&str, &[&str])] = &[
    ("flowchart", &["flowchart", "flowchart-elk"]),
    ("graph", &["graph"]),
    ("sequencediagram", &["sequenceDiagram"]),
    ("classdiagram", &["classDiagram", "classDiagram-v2"]),
    ("statediagram", &["stateDiagram", "stateDiagram-v2"]),
    ("erdiagram", &["erDiagram"]),
    ("pie", &["pie"]),
    ("mindmap", &["mindmap"]),
    ("journey", &["journey"]),
    ("timeline", &["timeline"]),
    ("gantt", &["gantt"]),
    ("requirementdiagram", &["requirementDiagram"]),
    ("gitgraph", &["gitGraph"]),
    ("c4", &["C4Context"]),
    ("c4context", &["C4Context"]),
    ("c4container", &["C4Container"]),
    ("c4component", &["C4Component"]),
    ("c4dynamic", &["C4Dynamic"]),
    ("c4deployment", &["C4Deployment"]),
    ("sankey", &["sankey-beta", "sankey"]),
    ("quadrantchart", &["quadrantChart"]),
    ("zenuml", &["zenuml"]),
    ("block", &["block-beta", "block"]),
    ("packet", &["packet-beta", "packet"]),
    ("kanban", &["kanban"]),
    ("architecture", &["architecture-beta", "architecture"]),
    ("radar", &["radar-beta", "radar"]),
    ("treemap", &["treemap-beta", "treemap"]),
    ("xychart", &["xychart-beta", "xychart"]),
];

/// What mmdr makes of a Mermaid diagram's first line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
    /// It names a kind ([`kind_of`]).
    Kind,
    /// It names none, and mmdr reads it as a flowchart's line: a flowchart
    /// written without its head (`A --> B`, `subgraph back-end`), drawn as
    /// one, which Mermaid itself is not.
    Flowchart,
    /// It names none and is refused on its line: a line mmdr reads as
    /// nothing, or a word where a kind is named with a hyphen in it, as
    /// Mermaid 11.16's `swimlane-beta LR` or `venn-beta`, which mmdr would
    /// read as a flowchart's first nodes and draw what is not there.
    Unknown,
}

/// The index of the line that names the kind among `lines`: past blank
/// lines, a byte order mark, front matter, the init directive and
/// comments, wherever they stand before it, as mmdr finds it.
pub fn head_line(lines: &[&str]) -> Option<usize> {
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim_start_matches('\u{feff}').trim();
        if t == "---" {
            i += 1;
            while i < lines.len() && lines[i].trim() != "---" {
                i += 1;
            }
            i += 1;
            continue;
        }
        if t.starts_with("%%{") {
            while i < lines.len() && !lines[i].contains("}%%") {
                i += 1;
            }
            i += 1;
            continue;
        }
        if t.is_empty() || t.starts_with("%%") {
            i += 1;
            continue;
        }
        return Some(i);
    }
    None
}

/// The word mmdr reads `header` by, lowercased: the kind it names.
pub fn kind_of(header: &str) -> Option<&'static str> {
    let lower = read(header).to_ascii_lowercase();
    KINDS.iter().map(|&(word, _)| word).find(|word| {
        lower.strip_prefix(word).is_some_and(|rest| {
            rest.chars()
                .next()
                .is_none_or(|c| !c.is_ascii_alphanumeric())
        })
    })
}

/// What mmdr makes of `header`, a Mermaid diagram's first line.
pub fn head_of(header: &str) -> Head {
    if kind_of(header).is_some() {
        return Head::Kind;
    }
    let line = read(header);
    let masked = masked(&line);
    let (word, rest) = match line.find(char::is_whitespace) {
        Some(at) => (&line[..at], &masked[at..]),
        None => (line.as_str(), ""),
    };
    let word = word.trim_end_matches([':', ';']);
    // A kind's word with a hyphen: letters on both sides of each, and no
    // more to the line that mmdr would take as a line between boxes.
    let kind_like = word.contains('-')
        && word
            .split('-')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_'))
        && !arrow(rest);
    if !kind_like && arrow(&masked) {
        Head::Flowchart
    } else {
        Head::Unknown
    }
}

/// What is said of a Mermaid diagram whose first line, `header`, names no
/// kind ([`Head::Unknown`]), by lint and by render alike.
pub fn unknown_said(header: &str) -> String {
    format!(
        "`{}` names no kind of diagram Omaspec draws: a Mermaid diagram's first line is its kind, as `flowchart LR` or `sequenceDiagram`",
        read(header)
    )
}

/// The kind `header` names, written otherwise than mermaid.js takes it
/// (`SequenceDiagram`, `xychart` without its `-beta`), and as mermaid.js
/// writes it: Omaspec draws the diagram, GitHub does not.
pub fn misspelled(header: &str) -> Option<(String, &'static str)> {
    let word = kind_of(header)?;
    let (_, written) = KINDS.iter().find(|(w, _)| *w == word)?;
    let line = read(header);
    let token = line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches([':', ';']);
    (!written.contains(&token)).then(|| (token.to_owned(), written[0]))
}

/// The line as mmdr reads it: without a byte order mark and without a `%%`
/// comment at its end, outside quotes.
fn read(line: &str) -> String {
    let line = line.trim_start_matches('\u{feff}');
    let mut quote: Option<char> = None;
    let mut chars = line.chars().peekable();
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            out.push(c);
            continue;
        }
        if c == '"' || c == '\'' {
            quote = Some(c);
            out.push(c);
            continue;
        }
        if c == '%' && chars.peek() == Some(&'%') {
            break;
        }
        out.push(c);
    }
    out.trim().to_owned()
}

/// `line` with what stands in brackets or quotes blanked, byte for byte:
/// a `(` or `{` opens only outside other brackets, a quote escaped with a
/// backslash is none.
fn masked(line: &str) -> String {
    let (mut square, mut paren, mut curly) = (0usize, 0usize, 0usize);
    let (mut double, mut single) = (false, false);
    let mut before = '\0';
    let mut out = String::with_capacity(line.len());
    let blank = |out: &mut String, c: char| out.extend(std::iter::repeat_n(' ', c.len_utf8()));
    for c in line.chars() {
        let inside = square > 0 || paren > 0 || curly > 0;
        let quoted = double || single;
        match c {
            '[' if !quoted => {
                square += 1;
                out.push(c);
            }
            ']' if !quoted && square > 0 => {
                square -= 1;
                out.push(c);
            }
            '(' if !quoted && !inside => {
                paren += 1;
                out.push(c);
            }
            ')' if !quoted && paren > 0 => {
                paren -= 1;
                out.push(c);
            }
            '{' if !quoted && !inside => {
                curly += 1;
                out.push(c);
            }
            '}' if !quoted && curly > 0 => {
                curly -= 1;
                out.push(c);
            }
            '"' if before != '\\' => {
                double = !double;
                if inside || quoted {
                    blank(&mut out, c);
                } else {
                    out.push(c);
                }
            }
            '\'' if before != '\\' => {
                single = !single;
                if inside || quoted {
                    blank(&mut out, c);
                } else {
                    out.push(c);
                }
            }
            _ if inside || quoted => blank(&mut out, c),
            _ => out.push(c),
        }
        before = c;
    }
    out
}

/// Whether mmdr finds a line between boxes in `masked`: any `-` or `=`
/// (its `ARROW_TOKEN_RE` takes one alone).
fn arrow(masked: &str) -> bool {
    masked.contains(['-', '='])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_first_line_is_what_mmdr_makes_of_it() {
        for kind in [
            "flowchart LR",
            "graph",
            "Flowchart LR",
            "flowchart_x LR",
            "block",
            "xychart-beta",
            "\u{feff}flowchart LR",
            "gitGraph:",
            "C4Context",
        ] {
            assert_eq!(head_of(kind), Head::Kind, "{kind}");
        }
        for line in [
            "A --> B",
            "subgraph back-end",
            "classDef hot fill:#f96,stroke-width:4px",
            "A[Step (draft] --> B",
            "back-end --> db",
        ] {
            assert_eq!(head_of(line), Head::Flowchart, "{line}");
        }
        for line in [
            "swimlane-beta LR",
            "swimlane-beta: LR",
            "venn-beta;",
            "ablauf-übersicht LR",
            "swimlane-beta LR %% see re-design",
            "A[Start]",
            "A[Start] %% to-do",
            "A ~~~ B",
            "A's x-y",
            "info",
        ] {
            assert_eq!(head_of(line), Head::Unknown, "{line}");
        }
        assert_eq!(
            unknown_said("swimlane-beta LR %% see re-design"),
            unknown_said("swimlane-beta LR")
        );
    }

    #[test]
    fn a_kind_written_otherwise_than_mermaid_js_takes_it_is_found() {
        assert_eq!(
            misspelled("SequenceDiagram"),
            Some(("SequenceDiagram".to_owned(), "sequenceDiagram"))
        );
        assert_eq!(
            misspelled("XYChart-beta"),
            Some(("XYChart-beta".to_owned(), "xychart-beta"))
        );
        assert_eq!(misspelled("xychart"), None);
        assert_eq!(
            misspelled("flowchart_x LR"),
            Some(("flowchart_x".to_owned(), "flowchart"))
        );
        for written in [
            "flowchart LR",
            "flowchart-elk TD",
            "graph TD;",
            "stateDiagram-v2",
            "gitGraph:",
            "xychart-beta",
            "C4Context",
        ] {
            assert_eq!(misspelled(written), None, "{written}");
        }
    }

    #[test]
    fn the_head_stands_past_what_comes_before_it() {
        let lines = [
            "\u{feff}",
            "%% a note",
            "---",
            "title: x",
            "---",
            "%%{init: {}}%%",
            "flowchart LR",
        ];
        assert_eq!(head_line(&lines), Some(6));
        assert_eq!(head_line(&["%% only a note"]), None);
    }
}
