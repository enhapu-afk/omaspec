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
    /// `@startflow`, Omagram's own signal-flow diagram.
    Flow,
    /// `@startjson`.
    Json,
    /// `@startlatex`.
    Latex,
    /// `@startmath`.
    Math,
}

/// One line of the diagram's body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Line<'a> {
    /// Zero-based line in the whole source.
    pub no: usize,
    /// The text, trimmed, with comments taken out.
    pub text: &'a str,
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
            let kind = match word.to_ascii_lowercase().as_str() {
                "startuml" => Kind::Uml,
                "startmindmap" => Kind::MindMap,
                "startwbs" => Kind::Wbs,
                "startgantt" => Kind::Gantt,
                "startflow" => Kind::Flow,
                "startjson" => Kind::Json,
                "startlatex" => Kind::Latex,
                "startmath" => Kind::Math,
                _ => {
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
        if text.starts_with("/'") {
            return Err(Declined::at(
                no,
                "a block comment after a block comment is not supported",
            ));
        }
        lines.push(Line {
            no,
            text,
            indent,
            blank_before: std::mem::take(&mut blanks),
        });
    }
    Ok((kind, lines))
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
fn trim(line: &str) -> &str {
    line.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// Whether `text` starts with the ASCII word `prefix`, ignoring case.
pub fn starts_with_ci(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let err = body("@startsalt\n{+\n}\n@endsalt\n").unwrap_err();
        assert_eq!(err.line, Some(0));
        assert!(err.reason.contains("@startsalt"), "{}", err.reason);
        assert_eq!(
            body("@startgantt\n[a] lasts 1 day\n@endgantt\n").unwrap().0,
            Kind::Gantt
        );
        assert_eq!(body("@startflow\nin a\n@endflow\n").unwrap().0, Kind::Flow);
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
