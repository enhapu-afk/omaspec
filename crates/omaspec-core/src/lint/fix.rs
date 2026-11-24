//! How a finding is put right: lines taken out and lines put in their
//! place. Only findings whose fix is sure carry one, so a fix can be
//! applied without being read first.

use serde::Serialize;

use crate::text::re;

/// Lines taken out at a line, and the lines put there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fix {
    /// What it does, to offer it: `Put the label in quotes`.
    pub title: String,
    /// Zero-based line where it starts; the count of lines puts lines in
    /// at the end.
    pub line: usize,
    /// How many lines it takes out there; 0 only puts lines in, before
    /// `line`.
    pub remove: usize,
    /// The lines it puts in, without line ends.
    pub insert: Vec<String>,
}

impl Fix {
    /// One line written anew.
    pub fn replace(title: impl Into<String>, line: usize, with: impl Into<String>) -> Fix {
        Fix {
            title: title.into(),
            line,
            remove: 1,
            insert: vec![with.into()],
        }
    }

    /// A line put in before `line`.
    pub fn insert(title: impl Into<String>, line: usize, text: impl Into<String>) -> Fix {
        Fix {
            title: title.into(),
            line,
            remove: 0,
            insert: vec![text.into()],
        }
    }

    /// One line taken out.
    pub fn remove(title: impl Into<String>, line: usize) -> Fix {
        Fix {
            title: title.into(),
            line,
            remove: 1,
            insert: Vec::new(),
        }
    }

    /// `code` with the fix made. The lines put in end as their neighbors
    /// do (`\r\n` stays `\r\n`), and a line put in after the last one
    /// comes before the empty end a final line break leaves.
    #[must_use]
    pub fn apply(&self, code: &str) -> String {
        let mut lines: Vec<&str> = code.split('\n').collect();
        // The text after the last line break is no line when it is empty.
        let end = match lines.last() {
            Some(last) if last.is_empty() && lines.len() > 1 => lines.len() - 1,
            _ => lines.len(),
        };
        let at = self.line.min(end);
        let until = (at + self.remove).min(end);
        let returns = lines[..end].iter().any(|line| line.ends_with('\r'));
        let put: Vec<String> = self
            .insert
            .iter()
            .map(|line| match returns {
                true => format!("{line}\r"),
                false => line.clone(),
            })
            .collect();
        // A line put in after a last line without a line break.
        let open = at == lines.len() && self.remove == 0;
        lines.splice(at..until, put.iter().map(String::as_str));
        let mut out = lines.join("\n");
        if open && returns {
            out.truncate(out.len() - 1);
        }
        out
    }
}

/// `code` with every fix of `findings` made, and how many were: from the
/// last line up, so the lines above stay where the findings saw them. Of
/// several fixes that take out the same line the first one is made, and
/// a fix said twice is made once.
pub fn fix_all(code: &str, findings: &[super::Finding]) -> (String, usize) {
    let mut fixes: Vec<&Fix> = findings.iter().filter_map(|f| f.fix.as_ref()).collect();
    // From the bottom; at one line, what takes it out before what is put
    // in before it.
    fixes.sort_by(|a, b| b.line.cmp(&a.line).then(b.remove.cmp(&a.remove)));
    let mut out = code.to_owned();
    let mut done = 0;
    // The first line a fix made so far has changed.
    let mut changed = usize::MAX;
    let mut put: Vec<&Fix> = Vec::new();
    for fix in fixes {
        if fix.remove > 0 {
            if fix.line + fix.remove > changed {
                continue;
            }
            changed = fix.line;
        } else if put.contains(&fix) {
            continue;
        } else {
            put.push(fix);
        }
        out = fix.apply(&out);
        done += 1;
    }
    (out, done)
}

/// `line` with `name` written as `with`, where `name` stands in it once
/// as a whole word; `None` where it stands twice or not at all, and a
/// fix would have to guess which.
pub(super) fn renamed(line: &str, name: &str, with: &str) -> Option<String> {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    let mut at = line.match_indices(name).filter(|&(i, _)| {
        !word(line[..i].chars().next_back()) && !word(line[i + name.len()..].chars().next())
    });
    let (first, _) = at.next()?;
    at.next()
        .is_none()
        .then(|| format!("{}{with}{}", &line[..first], &line[first + name.len()..]))
}

/// Where a block opened on `row` is closed by a fix: before the `@end…`
/// that ends its diagram, or the `@start…` of the next one where its own
/// end is missing, else after the last line, where PlantUML and Mermaid
/// end it now.
fn closing_line(lines: &[&str], row: usize) -> usize {
    first_after(lines, row, re!(r"(?i)^@(end|start)"))
}

/// Where a diagram started on `row` without its `@end…` gets one: before
/// the next diagram, else after the last line.
pub(super) fn diagram_end(lines: &[&str], row: usize) -> usize {
    first_after(lines, row, re!(r"(?i)^@start"))
}

/// The first line after `row` that `pattern` finds, trimmed; else the
/// line after the last.
fn first_after(lines: &[&str], row: usize, pattern: &regex::Regex) -> usize {
    lines
        .iter()
        .enumerate()
        .skip(row + 1)
        .find(|(_, l)| pattern.is_match(l.trim()))
        .map_or_else(|| text_end(lines), |(i, _)| i)
}

/// The line after the last: the empty end a final line break leaves is
/// no line.
pub(super) fn text_end(lines: &[&str]) -> usize {
    match lines.last() {
        Some(last) if last.is_empty() && lines.len() > 1 => lines.len() - 1,
        _ => lines.len(),
    }
}

/// The fix that closes the block `kind` opened on `row` with `closer`,
/// as deep as its opening line.
pub(super) fn closed(lines: &[&str], row: usize, kind: &str, closer: &str) -> Fix {
    let opening = lines.get(row).copied().unwrap_or_default();
    let lead = &opening[..opening.len() - opening.trim_start().len()];
    Fix::insert(
        format!("End the {kind} of line {}", row + 1),
        closing_line(lines, row),
        format!("{lead}{closer}"),
    )
}

/// The kind of diagram the first word of `header` misspells, as
/// mermaid.js writes it: the one nearest, an edit away (two for a longer
/// word), where no other is as near; with the word as written.
pub(super) fn kind_meant(header: &str) -> Option<(String, &'static str)> {
    let word = crate::mermaid_head::first_word(header);
    let low: Vec<char> = word.to_lowercase().chars().collect();
    if low.len() < 3 {
        return None;
    }
    let max = if low.len() > 5 { 2 } else { 1 };
    let mut near: Vec<(usize, &str)> = crate::mermaid_head::written_kinds()
        .map(|kind| {
            let kind_low: Vec<char> = kind.to_lowercase().chars().collect();
            (super::distance(&low, &kind_low, max), kind)
        })
        .filter(|&(d, _)| d <= max)
        .collect();
    near.sort_unstable();
    near.dedup_by_key(|(_, kind)| *kind);
    match near.as_slice() {
        [(_, kind)] => Some((word, kind)),
        [(a, kind), (b, _), ..] if a < b => Some((word, kind)),
        _ => None,
    }
}

/// The blocks left open in a diagram lie one inside another and all
/// close where it ends: the outermost one's fix closes them all, the
/// innermost first, so no block is left open inside the one it closes;
/// those inside carry none, so that each fix a reader makes is made once.
/// A block with no sure closer (a `repeat`) leaves those around it
/// unfixed.
pub(super) fn nested(findings: &mut [super::Finding]) {
    let mut ends: Vec<usize> = findings
        .iter()
        .filter(|f| f.code == "unclosed-block")
        .filter_map(|f| f.fix.as_ref().map(|fix| fix.line))
        .collect();
    ends.sort_unstable();
    ends.dedup();
    for end in ends {
        // Those that close there, or would, the innermost first.
        let mut open: Vec<usize> = (0..findings.len())
            .filter(|&i| {
                let f = &findings[i];
                f.code == "unclosed-block"
                    && f.line < end
                    && f.fix.as_ref().is_none_or(|fix| fix.line == end)
            })
            .collect();
        open.sort_by_key(|&i| std::cmp::Reverse(findings[i].line));
        let closable = open
            .iter()
            .position(|&i| findings[i].fix.is_none())
            .unwrap_or(open.len());
        let (inner, outer) = open.split_at(closable);
        if inner.len() > 1 {
            let insert: Vec<String> = inner
                .iter()
                .flat_map(|&i| findings[i].fix.as_ref().map(|f| f.insert.clone()))
                .flatten()
                .collect();
            let fix = Fix {
                title: format!("End the {} blocks left open", inner.len()),
                line: end,
                remove: 0,
                insert,
            };
            for &i in inner {
                findings[i].fix = None;
            }
            if let Some(&outermost) = inner.last() {
                findings[outermost].fix = Some(fix);
            }
        }
        for &i in outer {
            findings[i].fix = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::{Finding, Severity};

    #[test]
    fn a_fix_takes_lines_out_and_puts_lines_in() {
        let code = "a\nb\nc\n";
        assert_eq!(Fix::replace("", 1, "B").apply(code), "a\nB\nc\n");
        assert_eq!(Fix::remove("", 0).apply(code), "b\nc\n");
        assert_eq!(Fix::insert("", 1, "x").apply(code), "a\nx\nb\nc\n");
        // At the end: before the empty end, or after a last line left open.
        assert_eq!(Fix::insert("", 3, "x").apply(code), "a\nb\nc\nx\n");
        assert_eq!(Fix::insert("", 9, "x").apply(code), "a\nb\nc\nx\n");
        assert_eq!(Fix::insert("", 2, "x").apply("a\nb"), "a\nb\nx");
        assert_eq!(Fix::remove("", 7).apply(code), code);
        assert_eq!(Fix::insert("", 0, "x").apply(""), "x\n");
    }

    #[test]
    fn the_lines_put_in_end_as_their_neighbors_do() {
        let code = "a\r\nb\r\n";
        assert_eq!(Fix::replace("", 1, "B").apply(code), "a\r\nB\r\n");
        assert_eq!(Fix::insert("", 2, "x").apply(code), "a\r\nb\r\nx\r\n");
        assert_eq!(Fix::insert("", 2, "x").apply("a\r\nb"), "a\r\nb\nx");
    }

    #[test]
    fn every_fix_is_made_from_the_bottom_and_one_for_a_line() {
        let found = |fix: Fix| Finding::new("fix", fix.line, Severity::Info, "").with(fix);
        let findings = [
            found(Fix::replace("", 1, "B")),
            found(Fix::replace("", 1, "other")),
            found(Fix::insert("", 1, "x")),
            found(Fix::insert("", 1, "x")),
            found(Fix::remove("", 2)),
            found(Fix::insert("", 3, "end")),
            Finding::new("nothing-to-fix", 0, Severity::Error, "nothing to do"),
        ];
        assert_eq!(
            fix_all("a\nb\nc\n", &findings),
            ("a\nx\nB\nend\n".to_owned(), 4)
        );
        assert_eq!(fix_all("a\n", &findings[6..]), ("a\n".to_owned(), 0));
    }
    #[test]
    fn a_name_is_renamed_where_it_stands_once_as_a_word() {
        assert_eq!(
            renamed("Alcie -> Bob : hi", "Alcie", "Alice").as_deref(),
            Some("Alice -> Bob : hi")
        );
        assert_eq!(
            renamed("Alcie -> Alcie2", "Alcie", "Alice").as_deref(),
            Some("Alice -> Alcie2")
        );
        assert_eq!(renamed("Alcie -> Bob : Alcie", "Alcie", "Alice"), None);
        assert_eq!(renamed("Bob -> Carol", "Alcie", "Alice"), None);
    }

    #[test]
    fn a_block_is_closed_before_the_end_of_its_diagram() {
        let lines = ["@startuml", "  alt ok", "    A -> B", "@enduml", ""];
        let fix = closed(&lines, 1, "alt", "end");
        assert_eq!(
            (fix.line, fix.insert.as_slice()),
            (3, &["  end".to_owned()][..])
        );
        assert_eq!(fix.title, "End the alt of line 2");
        // Without an end, after the last line.
        assert_eq!(closed(&lines[..3], 1, "alt", "end").line, 3);
    }

    #[test]
    fn a_kind_misspelled_is_the_one_nearest() {
        assert_eq!(
            kind_meant("sequenceDiagam"),
            Some(("sequenceDiagam".to_owned(), "sequenceDiagram"))
        );
        assert_eq!(kind_meant("flowchar LR").map(|(_, k)| k), Some("flowchart"));
        assert_eq!(kind_meant("nothing at all"), None);
        assert_eq!(kind_meant("pi"), None);
    }
}
