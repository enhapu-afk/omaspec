//! What Omaspec reads of a Mermaid diagram before a drawer does: the line
//! that names its kind and its `%%{init}%%` directive, as mmdr reads them,
//! so a diagram Omaspec draws itself does not wait on mmdr's parse.
//!
//! Front matter between `---` lines is skipped, and so are `%%` comments,
//! as mmdr skips them: a `---` line opens and closes it wherever it is.

use mermaid_rs_renderer::ParseError;
use serde_json::Value;

/// The line that names the diagram's kind, in lower case: the first that
/// is not empty, front matter or a comment.
pub(super) fn header(body: &str) -> Option<String> {
    significant(body)
        .map(|(_, _, line)| line)
        .find(|line| !line.starts_with("%%"))
        .map(str::to_ascii_lowercase)
}

/// Whether `header` starts with `word` as a word of its own:
/// `stateDiagram-v2` names `statediagram`, `pieChart` does not name `pie`.
pub(super) fn names(header: &str, word: &str) -> bool {
    header.strip_prefix(word).is_some_and(|rest| {
        rest.chars()
            .next()
            .is_none_or(|c| !c.is_ascii_alphanumeric())
    })
}

/// The settings of the last `%%{init: …}%%` directive, as JSON or JSON5.
/// A directive that is not well formed is the error mmdr gives on its
/// line; another directive than `init` is left alone.
pub(super) fn init(body: &str) -> Result<Option<Value>, ParseError> {
    let mut found = None;
    for (index, raw, line) in significant(body) {
        let Some(rest) = line.strip_prefix("%%{") else {
            continue;
        };
        let invalid = |directive: &str, reason: String| ParseError::InvalidDirective {
            line: u32::try_from(index + 1).unwrap_or(u32::MAX),
            col: u32::try_from(raw.chars().take_while(|c| c.is_whitespace()).count() + 1)
                .unwrap_or(u32::MAX),
            directive: directive.to_owned(),
            reason,
        };
        let Some(inside) = rest.strip_suffix("}%%") else {
            return Err(invalid("unknown", "missing closing '}%%' fence".into()));
        };
        let Some((name, settings)) = inside.split_once(':') else {
            return Err(invalid(
                "unknown",
                "missing ':' between directive name and body".into(),
            ));
        };
        let (name, settings) = (name.trim(), settings.trim());
        if name != "init" {
            continue;
        }
        if settings.is_empty() {
            return Err(invalid("init", "empty body".into()));
        }
        let value = serde_json::from_str::<Value>(settings)
            .or_else(|_| json5::from_str::<Value>(settings))
            .map_err(|err| invalid("init", format!("JSON parse error: {err}")))?;
        found = Some(value);
    }
    Ok(found)
}

/// The lines outside the front matter that are not empty: each with its
/// index, as written, and trimmed.
fn significant(body: &str) -> impl Iterator<Item = (usize, &str, &str)> {
    let mut front = false;
    body.lines().enumerate().filter_map(move |(index, raw)| {
        let line = raw.trim();
        if line.is_empty() {
            return None;
        }
        if line == "---" {
            front = !front;
            return None;
        }
        (!front).then_some((index, raw, line))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_header_comes_after_front_matter_and_comments() {
        let body = "---\ntitle: pie\n---\n%% a pie\n\n  stateDiagram-v2 %% done\n  A\n";
        let header = header(body).unwrap();
        assert!(names(&header, "statediagram"));
        assert!(!names(&header, "state"));
        assert!(!names("piechart", "pie"));
        assert!(names("pie title Pets", "pie"));
        assert_eq!(super::header("%% only a comment\n"), None);
    }

    #[test]
    fn the_last_init_counts_and_json5_is_read() {
        let body = "%%{init: {\"theme\": \"dark\"}}%%\n%%{ init : {'theme': 'forest'} }%%\n%%{wrap: true}%%\npie\n";
        let init = init(body).unwrap().unwrap();
        assert_eq!(init["theme"], "forest");
        assert_eq!(super::init("pie\n").unwrap(), None);
        // In front matter it is not a directive.
        assert_eq!(super::init("---\n%%{init: x}%%\n---\npie\n").unwrap(), None);
    }

    #[test]
    fn a_broken_init_is_an_error_on_its_line() {
        let err = init("pie\n  %%{init: {theme: }}%%\n").unwrap_err();
        assert!(
            matches!(
                err,
                ParseError::InvalidDirective {
                    line: 2,
                    col: 3,
                    ..
                }
            ),
            "{err}"
        );
        assert!(init("%%{init: {}\n").is_err());
        assert!(init("%%{init}%%\n").is_err());
        assert!(init("%%{init: }%%\n").is_err());
    }
}
