//! The lines of a diagram as the engine sees them: the body between
//! `@start…` and `@end…`, without blank lines and comments, each with its
//! line number in the whole source.

use crate::Declined;

/// The diagram types the engine knows, by their start line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `@startuml`, or no start line: the type follows from the lines.
    Uml,
    /// `@startmindmap`.
    MindMap,
    /// `@startwbs`.
    Wbs,
    /// `@startgantt`.
    Gantt,
    /// `@startchart`: bars, lines, areas and points over two axes.
    Chart,
    /// `@startflow`, Omaspec's own signal-flow diagram.
    Flow,
    /// `@startstorymap`, Omaspec's own story map.
    StoryMap,
    /// `@startcapabilities`, Omaspec's own capability map.
    Capabilities,
    /// `@startjson`.
    Json,
    /// `@startyaml`: drawn as the JSON it stands for.
    Yaml,
    /// `@startnwdiag`: a network diagram.
    Network,
    /// `@startsalt`: a wireframe.
    Salt,
    /// `@startebnf`: a grammar as railroads.
    Ebnf,
    /// `@startregex`: a regular expression as a railroad.
    Regex,
    /// `@startditaa`: a drawing in characters.
    Ditaa,
    /// `@startlatex`.
    Latex,
    /// `@startmath`.
    Math,
}

/// The word after `@start` of each type the engine draws.
const KINDS: &[(&str, Kind)] = &[
    ("uml", Kind::Uml),
    ("mindmap", Kind::MindMap),
    ("wbs", Kind::Wbs),
    ("gantt", Kind::Gantt),
    ("chart", Kind::Chart),
    ("flow", Kind::Flow),
    ("storymap", Kind::StoryMap),
    ("capabilities", Kind::Capabilities),
    ("json", Kind::Json),
    ("yaml", Kind::Yaml),
    ("nwdiag", Kind::Network),
    ("salt", Kind::Salt),
    ("ebnf", Kind::Ebnf),
    ("regex", Kind::Regex),
    ("ditaa", Kind::Ditaa),
    ("latex", Kind::Latex),
    ("math", Kind::Math),
];

/// The words after `@start` the engine draws (`uml`, `chart`, …), in
/// lowercase: what tells a text's language holds itself to them.
pub fn start_words() -> impl Iterator<Item = &'static str> {
    KINDS.iter().map(|(word, _)| *word)
}

/// One line of the diagram's body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// Zero-based line in the whole source.
    pub no: usize,
    /// The text, trimmed, with comments taken out.
    pub text: &'a str,
    /// The byte in its line of the source where `text` starts, as
    /// written: a reader that keeps where words stand on their line
    /// counts from here ([`crate::common::Text::col`]).
    pub at: usize,
    /// Spaces and tabs before the text, each one counting one: mind maps
    /// and WBS diagrams take levels from them.
    pub indent: usize,
    /// Blank lines right before this one (comments between them do not
    /// count): a block of text keeps them as empty lines.
    pub blank_before: usize,
}

/// The body of the first diagram in `source`, and its type. Without an
/// `@start` line the whole source is the body; without an `@end` line it
/// runs to the end.
pub fn body(source: &str) -> Result<(Kind, Vec<Line<'_>>), Declined> {
    let full: Vec<&str> = source
        .split('\n')
        .map(|l| l.trim_start_matches('\u{feff}'))
        .collect();
    let raw: Vec<&str> = full.iter().map(|l| trim(l)).collect();
    let (kind, start, end) = match raw.iter().position(|l| starts_with_ci(l, "@start")) {
        Some(at) => {
            let head = raw[at];
            let word = head[1..]
                .split(|c: char| c.is_whitespace() || c == '(')
                .next()
                .unwrap_or_default();
            let named = word
                .get("start".len()..)
                .and_then(|block| KINDS.iter().find(|(w, _)| w.eq_ignore_ascii_case(block)));
            let kind = match named {
                Some(&(_, kind)) => kind,
                None => {
                    return Err(Declined::at(
                        at,
                        format!("`@{word}` diagrams are drawn by the Java PlantUML only"),
                    ));
                }
            };
            let end = (at + 1..raw.len())
                .find(|&i| starts_with_ci(raw[i], "@end"))
                .unwrap_or(raw.len());
            (kind, at + 1, end)
        }
        None => (Kind::Uml, 0, raw.len()),
    };

    // Comments as PlantUML's preprocessor takes them out: a line that starts
    // with `'`, a line that is one `/' … '/`, and a block that a line
    // starting with `/'` and without `'/` opens. The block runs up to and
    // with the next line that ends with `'/`.
    let mut lines = Vec::new();
    let mut in_block = false;
    let mut blanks = 0;
    for (no, &text) in raw.iter().enumerate().take(end).skip(start) {
        if in_block {
            in_block = !text.ends_with("'/");
            continue;
        }
        if text.starts_with('\'') {
            continue;
        }
        // The same `@start…` written twice opens no second diagram; PlantUML
        // passes over it.
        if no == start
            && starts_with_ci(text, "@start")
            && text.eq_ignore_ascii_case(raw[start - 1])
        {
            continue;
        }
        if text.starts_with("/'") {
            if !text.contains("'/") {
                in_block = true;
                continue;
            }
            if text.len() >= 4 && text.ends_with("'/") {
                continue;
            }
        }
        let kept = inner_comment(text, no)?;
        // What a comment at the start leaves keeps its own indentation.
        let indent = if kept.as_ptr() == text.as_ptr() {
            leading_blanks(full[no])
        } else {
            leading_blanks(kept)
        };
        let text = trim(kept);
        if text.is_empty() {
            blanks += 1;
            continue;
        }
        if text.starts_with('\'') {
            continue;
        }
        // The same `@start…` written twice opens no second diagram; PlantUML
        // passes over it.
        if no == start
            && starts_with_ci(text, "@start")
            && text.eq_ignore_ascii_case(raw[start - 1])
        {
            continue;
        }
        if text.starts_with("/'") {
            return Err(Declined::at(
                no,
                "a block comment after a block comment is not supported",
            ));
        }
        lines.push(Line {
            no,
            text,
            at: offset_in(full[no], text),
            indent,
            blank_before: std::mem::take(&mut blanks),
        });
    }
    Ok((kind, lines))
}

/// The byte in `whole` where `part`, a slice of it, starts.
pub(crate) fn offset_in(whole: &str, part: &str) -> usize {
    (part.as_ptr() as usize)
        .saturating_sub(whole.as_ptr() as usize)
        .min(whole.len())
}

fn leading_blanks(line: &str) -> usize {
    line.chars().take_while(|&c| c == ' ' || c == '\t').count()
}

/// `text` without a block comment at one of its ends, like PlantUML: after
/// the first `'/` if it starts with `/'`, before the last `/'` if it ends
/// with `'/`. A comment in the middle stays, as text. Not trimmed.
fn inner_comment(text: &str, no: usize) -> Result<&str, Declined> {
    let kept = if text.starts_with("/'")
        && let Some(close) = text.find("'/")
    {
        &text[close + 2..]
    } else if text.ends_with("'/")
        && let Some(open) = text.rfind("/'")
    {
        &text[..open]
    } else {
        text
    };
    if has_special_comment(kept) {
        return Err(Declined::at(
            no,
            "a `/'''…'''/` comment inside a line is not supported",
        ));
    }
    Ok(kept)
}

/// Whether `text` has a `/'''word'''/`, which PlantUML cuts out anywhere.
fn has_special_comment(text: &str) -> bool {
    text.match_indices("/'''").any(|(at, _)| {
        let after = &text[at + 4..];
        let word = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-')))
            .unwrap_or(after.len());
        after[word..].starts_with("'''/")
    })
}

/// Whitespace, a carriage return and byte order marks off both ends.
///
/// One line keeps a no-break space: `title`, `header` or `footer` and
/// nothing after it but spaces with one of them among them. That is a
/// text that shows nothing, as in PlantUML, not the first line of a block
/// ([`shows_nothing`] reads it). Every other line ends with its last word.
fn trim(line: &str) -> &str {
    let blank = |c: char| c.is_whitespace() || c == '\u{feff}';
    let text = line.trim_matches(blank);
    let start = line.len() - line.trim_start_matches(blank).len();
    let after = &line[start + text.len()..];
    let frame = ["title", "header", "footer"]
        .iter()
        .any(|word| text.eq_ignore_ascii_case(word));
    match after.find(NO_BREAK_SPACE) {
        Some(at) if frame && at > 0 => {
            &line[start..start + text.len() + at + NO_BREAK_SPACE.len_utf8()]
        }
        _ => text,
    }
}

/// U+00A0: a space that is a text.
pub(crate) const NO_BREAK_SPACE: char = '\u{a0}';

/// Whether `text`, a line of the body that begins with a word for a
/// title, header or footer, gives it a no-break space as its whole text.
pub(crate) fn shows_nothing(text: &str) -> bool {
    text.ends_with(NO_BREAK_SPACE)
}

/// The `title:` of the front matter a Mermaid diagram may start with,
/// unquoted, and its zero-based line.
pub fn front_title(source: &str) -> Option<(usize, &str)> {
    let mut open = false;
    for (no, line) in source.split('\n').enumerate() {
        let line = line.trim();
        if line.is_empty() && !open {
            continue;
        }
        if line == "---" {
            if open {
                return None;
            }
            open = true;
            continue;
        }
        if !open {
            return None;
        }
        if let Some(title) = line.strip_prefix("title:") {
            let title = title.trim().trim_matches('"').trim();
            return (!title.is_empty()).then_some((no, title));
        }
    }
    None
}

/// The lines of a Mermaid front matter that every drawer draws as they
/// ask, past its `title`: a `config:` that holds only a `look`, `classic`
/// or `handDrawn`, which the drawer reads itself (docs/adr/0023), and the
/// settings of a diagram's own section its drawer carries out
/// ([`Front::with`]). Any other setting may change the picture, so a
/// reader that does not know it leaves the diagram to mmdr.
#[derive(Debug, Default)]
pub(crate) struct Front {
    /// How far `config:` is indented, while its keys follow.
    config: Option<usize>,
    /// The diagram's own section (`gitGraph`) and the keys of it its
    /// drawer carries out.
    section: Option<(&'static str, &'static [&'static str])>,
    /// How far that section is indented, while its keys follow.
    inside: Option<usize>,
}

impl Front {
    /// A front matter whose section `name` holds settings the drawer
    /// carries out: `keys`.
    pub(crate) fn with(name: &'static str, keys: &'static [&'static str]) -> Front {
        Front {
            section: Some((name, keys)),
            ..Front::default()
        }
    }

    /// Whether `raw`, a line of the front matter as written, is one of
    /// those, in the order the lines come.
    pub(crate) fn drawn(&mut self, raw: &str) -> bool {
        let text = raw.trim();
        let indent = raw.len() - raw.trim_start().len();
        if let Some(at) = self.inside {
            if indent > at {
                let keys = self.section.map_or(&[][..], |(_, keys)| keys);
                return text
                    .split_once(':')
                    .is_some_and(|(key, _)| keys.contains(&key.trim()));
            }
            self.inside = None;
        }
        if let Some(at) = self.config {
            if indent > at {
                if let Some((name, _)) = self.section
                    && text.strip_suffix(':').is_some_and(|key| key.trim() == name)
                {
                    self.inside = Some(indent);
                    return true;
                }
                return text.split_once(':').is_some_and(|(key, value)| {
                    let value = value.split(" #").next().unwrap_or_default();
                    let value = value.trim().trim_matches(['"', '\'']);
                    key.trim() == "look"
                        && (value.eq_ignore_ascii_case("classic")
                            || value.eq_ignore_ascii_case("handDrawn"))
                });
            }
            self.config = None;
        }
        if text == "config:" {
            self.config = Some(indent);
            return true;
        }
        false
    }
}

/// How many lines a Mermaid front matter at the top of `source` takes,
/// for a reader that draws no setting of it but those [`Front`] knows;
/// `Err` with the line of the first other setting.
pub(crate) fn front_lines(source: &str) -> Result<usize, usize> {
    let mut lines = source.lines().enumerate();
    if lines.next().is_none_or(|(_, raw)| raw.trim() != "---") {
        return Ok(0);
    }
    let mut front = Front::default();
    for (no, raw) in lines {
        let text = raw.trim();
        if text == "---" {
            return Ok(no + 1);
        }
        if !(text.is_empty() || text.starts_with('#') || front.drawn(raw)) {
            return Err(no);
        }
    }
    Ok(0)
}

/// Whether `text` starts with the ASCII word `prefix`, ignoring case.
pub fn starts_with_ci(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_front_matter_title_is_read_at_the_top_only() {
        let title = |s| front_title(s);
        assert_eq!(
            title("---\ntitle: Door\n---\nflowchart LR\n"),
            Some((1, "Door"))
        );
        assert_eq!(
            title("\n---\nconfig: x\ntitle: \"Two words\"\n---\n"),
            Some((3, "Two words"))
        );
        assert_eq!(title("---\n---\ntitle: late\n"), None);
        assert_eq!(title("flowchart LR\n---\ntitle: Not one\n---\n"), None);
        assert_eq!(title("---\ntitle:\n---\n"), None);
    }

    #[test]
    fn a_config_of_a_look_is_drawn_and_nothing_else_in_it() {
        let drawn = |lines: &[&str]| {
            let mut front = Front::default();
            lines
                .iter()
                .map(|line| front.drawn(line))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            drawn(&["config:", "  look: handDrawn", "title: x"]),
            [true, true, false]
        );
        assert_eq!(
            drawn(&["config:", "  look: 'classic' # plain"]),
            [true, true]
        );
        assert_eq!(
            drawn(&["config:", "  theme: dark", "  look: neo"]),
            [true, false, false]
        );
        assert_eq!(drawn(&["look: handDrawn"]), [false]);
        // A section of the diagram's own, with the keys its drawer knows.
        let mut front = Front::with("gitGraph", &["mainBranchName"]);
        let lines = [
            "config:",
            "  gitGraph:",
            "    mainBranchName: trunk",
            "    parallelCommits: true",
            "  look: handDrawn",
            "  flowchart:",
        ];
        let found: Vec<bool> = lines.iter().map(|line| front.drawn(line)).collect();
        assert_eq!(found, [true, true, true, false, true, false]);
    }

    #[test]
    fn a_front_matter_of_a_look_is_passed_over() {
        assert_eq!(front_lines("C4Context\n"), Ok(0));
        assert_eq!(
            front_lines("---\nconfig:\n  look: handDrawn\n---\nC4Context\n"),
            Ok(4)
        );
        assert_eq!(front_lines("---\ntitle: x\n---\nC4Context\n"), Err(1));
    }

    fn texts(source: &str) -> Vec<(usize, &str)> {
        body(source)
            .unwrap()
            .1
            .into_iter()
            .map(|l| (l.no, l.text))
            .collect()
    }

    #[test]
    fn takes_the_body_without_comments() {
        let source = "' file comment\n@startuml name\n  A -> B : hi\r\n' gone\n\n/' one line '/\nC -> D /' tail '/\n/' head '/ D -> C\n@enduml\nafter\n";
        assert_eq!(
            texts(source),
            [(2, "A -> B : hi"), (6, "C -> D"), (7, "D -> C")]
        );
    }

    #[test]
    fn block_comments_end_on_a_line_ending_with_the_close() {
        // The line that closes the block goes with it, also what comes
        // before the `'/`; a `'/` elsewhere does not close the block.
        let source =
            "A -> B\n/' block\n'/ B -> A\nstill comment '/\nC -> D\n  /' again\nlast '/  \nD -> C";
        assert_eq!(texts(source), [(0, "A -> B"), (4, "C -> D"), (7, "D -> C")]);
        // Without a line ending with `'/` the rest is comment.
        assert_eq!(texts("A -> B\n/' open\nB -> A '/ x"), [(0, "A -> B")]);
    }

    #[test]
    fn comments_inside_a_line_are_cut_only_at_its_ends() {
        // In the middle, `/'` is text, as in PlantUML.
        assert_eq!(
            texts("A -> B : see /' note\nB -> A : reply\nA -> B : a /' x '/ b"),
            [
                (0, "A -> B : see /' note"),
                (1, "B -> A : reply"),
                (2, "A -> B : a /' x '/ b"),
            ]
        );
        // At the end, the cut is at the last `/'`; at the start, after the
        // first `'/`.
        assert_eq!(
            texts("A -> B : a /' b /' c '/\n/' a '/ B -> A '/ b '/ x"),
            [(0, "A -> B : a /' b"), (1, "B -> A '/ b '/ x")]
        );
        let err = body("A -> B /'''note'''/ : x").unwrap_err();
        assert_eq!(err.line, Some(0));
    }

    #[test]
    fn without_start_the_whole_text_counts() {
        assert_eq!(texts("A -> B\n\nB -> A"), [(0, "A -> B"), (2, "B -> A")]);
        assert_eq!(texts("@startuml\nA -> B"), [(1, "A -> B")]);
    }

    #[test]
    fn knows_the_type_and_the_indentation() {
        let (kind, lines) =
            body("@startmindmap\n* a\n  ** b\n\t+ c\n/' x '/  * d\n@endmindmap\n").unwrap();
        assert_eq!(kind, Kind::MindMap);
        let indents: Vec<(&str, usize)> = lines.iter().map(|l| (l.text, l.indent)).collect();
        assert_eq!(indents, [("* a", 0), ("** b", 2), ("+ c", 1), ("* d", 2)]);
        let (_, lines) = body("a\n\n' x\n\nb").unwrap();
        assert_eq!((lines[1].text, lines[1].blank_before), ("b", 2));
        assert_eq!(body("@startWBS\n* a\n@endwbs").unwrap().0, Kind::Wbs);
        assert_eq!(body("A -> B").unwrap().0, Kind::Uml);
    }

    #[test]
    fn declines_other_blocks_and_keeps_directives() {
        let err = body("@startdot\ndigraph { a -> b }\n@enddot\n").unwrap_err();
        assert_eq!(err.line, Some(0));
        assert!(err.reason.contains("@startdot"), "{}", err.reason);
        assert_eq!(
            body("@startgantt\n[a] lasts 1 day\n@endgantt\n").unwrap().0,
            Kind::Gantt
        );
        assert_eq!(body("@startflow\nin a\n@endflow\n").unwrap().0, Kind::Flow);
        assert_eq!(
            body("@startstorymap\nactivity A\n@endstorymap\n")
                .unwrap()
                .0,
            Kind::StoryMap
        );
        assert_eq!(body("@startJSON\n{}\n@endjson\n").unwrap().0, Kind::Json);
        assert_eq!(body("@startLaTeX\na\n@endlatex\n").unwrap().0, Kind::Latex);
        assert_eq!(body("@startMATH\na\n@endmath\n").unwrap().0, Kind::Math);
        // Directives go on to the preprocessor.
        assert_eq!(
            texts("@startuml\n!define X 1\n@enduml\n"),
            [(1, "!define X 1")]
        );
    }
}
