//! How a finding is put right: lines taken out and lines put in their
//! place. Only findings whose fix is sure carry one, so a fix can be
//! applied without being read first.

use serde::Serialize;

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
/// several fixes that take out the same line the first one is made.
pub fn fix_all(code: &str, findings: &[super::Finding]) -> (String, usize) {
    let mut fixes: Vec<&Fix> = findings.iter().filter_map(|f| f.fix.as_ref()).collect();
    // From the bottom; at one line, what takes it out before what is put
    // in before it.
    fixes.sort_by(|a, b| b.line.cmp(&a.line).then(b.remove.cmp(&a.remove)));
    let mut out = code.to_owned();
    let mut done = 0;
    // The first line a fix made so far has changed.
    let mut changed = usize::MAX;
    let mut put: Vec<(usize, &[String])> = Vec::new();
    for fix in fixes {
        if fix.remove > 0 {
            if fix.line + fix.remove > changed {
                continue;
            }
            changed = fix.line;
        } else if put.contains(&(fix.line, fix.insert.as_slice())) {
            continue;
        } else {
            put.push((fix.line, &fix.insert));
        }
        out = fix.apply(&out);
        done += 1;
    }
    (out, done)
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
        let found = |fix: Fix| Finding::new(fix.line, Severity::Info, "").with(fix);
        let findings = [
            found(Fix::replace("", 1, "B")),
            found(Fix::replace("", 1, "other")),
            found(Fix::insert("", 1, "x")),
            found(Fix::insert("", 1, "x")),
            found(Fix::remove("", 2)),
            found(Fix::insert("", 3, "end")),
            Finding::new(0, Severity::Error, "nothing to do"),
        ];
        assert_eq!(
            fix_all("a\nb\nc\n", &findings),
            ("a\nx\nB\nend\n".to_owned(), 4)
        );
        assert_eq!(fix_all("a\n", &findings[6..]), ("a\n".to_owned(), 0));
    }
}
