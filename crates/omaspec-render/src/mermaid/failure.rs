//! What mmdr's parse errors mean, said of the line they are about.
//!
//! mmdr's own parser fails with a message and no place. Its strict parse
//! hands that on as an unexpected token at 1:1, the token being the first
//! line of the text: front matter, a comment or Omaspec's `%% skin:` line,
//! none of which is to blame. Such an error is said again here: the
//! message alone, on the line it names, and a header no kind answers to
//! as that.

use mermaid_rs_renderer::ParseError;

/// The zero-based line of `body` that `err` is about, if one is, and what
/// to say.
pub(super) fn said(err: &ParseError, body: &str) -> (Option<usize>, String) {
    match err {
        ParseError::UnexpectedToken {
            line: 1,
            col: 1,
            found,
            expected,
        } if *found == first_line(body) => unplaced(expected, body),
        _ => (super::error_line(err), err.to_string()),
    }
}

/// The token mmdr's strict parse names for an error its parser gave no
/// place: the first line that is not blank, trimmed.
fn first_line(body: &str) -> &str {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("<empty>")
}

/// An error of mmdr's parser, which names no line: on the line it is
/// about, where the message tells which.
fn unplaced(message: &str, body: &str) -> (Option<usize>, String) {
    if message == "unknown or missing Mermaid diagram header" {
        return match super::head::header_at(body) {
            None => (None, "the diagram is empty".to_owned()),
            Some((at, header)) => (
                Some(at),
                format!(
                    "`{header}` names no kind of diagram Omaspec draws: a Mermaid diagram's first line is its kind, as `flowchart LR` or `sequenceDiagram`"
                ),
            ),
        };
    }
    if let Some(edge) = message.strip_prefix("invalid flowchart edge syntax: ") {
        let at = body.lines().position(|line| line.contains(edge.trim()));
        return (at, format!("`{}` is no edge mmdr can read", edge.trim()));
    }
    (None, message.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said_of(body: &str) -> (Option<usize>, String) {
        let err = mermaid_rs_renderer::parse_mermaid_strict(body).expect_err("an error");
        said(&err, body)
    }

    #[test]
    fn an_error_without_a_place_is_said_on_its_own_line() {
        // Not on the front matter, a comment or the skin's line.
        for head in ["", "---\ntitle: X\n---\n", "%% skin: omarchy (Omaspec)\n"] {
            let body = format!("{head}flowchart TD\n  A --> B\n  B --> C -->\n");
            let (line, message) = said_of(&body);
            let lines: Vec<&str> = body.lines().collect();
            assert_eq!(lines[line.unwrap()].trim(), "B --> C -->", "{head}");
            assert_eq!(message, "`B --> C -->` is no edge mmdr can read");
        }
        let (line, message) = said_of("%% skin: plain (Omaspec)\n\nhello world\n");
        assert_eq!(line, Some(2));
        assert!(
            message.starts_with("`hello world` names no kind"),
            "{message}"
        );
        let (line, message) = said_of("---\ntitle: Only\n---\n%% a note\n");
        assert_eq!((line, message.as_str()), (None, "the diagram is empty"));
    }

    #[test]
    fn an_error_with_its_place_keeps_it() {
        let body = "flowchart TD\n  A --> B\n  end\n";
        let err = mermaid_rs_renderer::parse_mermaid_strict(body).unwrap_err();
        assert_eq!(said(&err, body), (Some(2), err.to_string()));
    }
}
