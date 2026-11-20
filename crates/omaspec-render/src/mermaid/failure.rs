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
        } if *found == first_line(body) && unplaced_message(expected) => unplaced(expected, body),
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

const NO_HEADER: &str = "unknown or missing Mermaid diagram header";
const BAD_EDGE: &str = "invalid flowchart edge syntax: ";

/// Whether `message` is one of mmdr's parser, which gives no place: its
/// validator's errors at 1:1 are on the first line indeed.
fn unplaced_message(message: &str) -> bool {
    message == NO_HEADER
        || message.starts_with(BAD_EDGE)
        || message.starts_with("invalid Mermaid init directive")
}

/// An error of mmdr's parser, which names no line: on the line it is
/// about, where the message tells which.
fn unplaced(message: &str, body: &str) -> (Option<usize>, String) {
    if message == NO_HEADER {
        return match super::head::header_at(body) {
            None => (None, "the diagram is empty".to_owned()),
            Some((at, header)) => (Some(at), omaspec_core::mermaid_head::unknown_said(header)),
        };
    }
    if let Some(edge) = message.strip_prefix(BAD_EDGE) {
        let edge = edge.trim();
        // The line that is the edge, else one that holds it (a chain of
        // edges, statements apart by `;`); never a comment or front matter.
        let lines: Vec<(usize, &str)> = super::head::significant(body)
            .filter(|(_, _, line)| !line.starts_with("%%"))
            .map(|(at, _, line)| (at, line))
            .collect();
        let at = lines
            .iter()
            .find(|(_, line)| *line == edge)
            .or_else(|| lines.iter().find(|(_, line)| line.contains(edge)))
            .map(|&(at, _)| at);
        return (at, format!("`{edge}` is no edge mmdr can read"));
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

    /// A head no kind answers to is refused on its line, not drawn as a
    /// flowchart whose first nodes are its words (REQ-089): Mermaid 11.16's
    /// `swimlane-beta`, before Omaspec drew it, came out as boxes `swimlane` and `beta`, its `LR`
    /// left aside. `graph` without a direction is a flowchart.
    #[test]
    fn a_head_no_kind_answers_to_is_refused_on_its_line() {
        let lanes = "%%{init: {\"theme\": \"dark\"}}%%\nfunnel-beta LR\n  stage Customer\n    A[Order]\n  stage Shop\n    B[Pack]\n  A --> B\n";
        for (body, line, head) in [
            (lanes, 1, "funnel-beta LR"),
            ("venn-beta\n  A\n", 0, "venn-beta"),
            ("funnel-beta: LR\n  A\n", 0, "funnel-beta: LR"),
            ("ablauf-übersicht LR\n  A\n", 0, "ablauf-übersicht LR"),
        ] {
            match super::super::render(body) {
                Err(crate::RenderError::Diagram {
                    line: at, message, ..
                }) => {
                    assert_eq!(at, Some(line), "{body}");
                    assert_eq!(message, omaspec_core::mermaid_head::unknown_said(head));
                }
                other => panic!("{body}: {other:?}"),
            }
        }
        // What mmdr takes as a kind is drawn, as its words end: without
        // `-beta`, in another case, after a word's end that is no letter.
        for body in [
            "graph\n  A --> B\n",
            "flowchart-elk TD\n  A --> B\n",
            "Flowchart LR\n  A --> B\n",
            "flowchart_x LR\n  A --> B\n",
            "block\n  a b\n",
            "\u{feff}flowchart LR\n  A --> B\n",
            "%% a note\n---\ntitle: x\n---\nflowchart LR\n  A --> B\n",
            // A flowchart written without its head: mmdr draws it, also
            // where a hyphen stands in a later word.
            "A --> B\n",
            "subgraph back-end\n  A --> B\nend\n",
            "classDef hot fill:#f96,stroke-width:4px\nA --> B\n",
        ] {
            assert!(super::super::render(body).is_ok(), "{body}");
        }
    }

    /// An edge written whole above the one that breaks, or in a comment,
    /// is not the one.
    #[test]
    fn the_edge_that_breaks_is_found_by_its_whole_line() {
        let body = "flowchart TD\n  %% A -->\n  A --> B\n  A -->\n";
        assert_eq!(said_of(body).0, Some(3));
    }

    #[test]
    fn an_error_with_its_place_keeps_it() {
        let body = "flowchart TD\n  A --> B\n  end\n";
        let err = mermaid_rs_renderer::parse_mermaid_strict(body).unwrap_err();
        assert_eq!(said(&err, body), (Some(2), err.to_string()));
        // On the first line, a validator's error is where it says.
        let body = "-->\n";
        let err = mermaid_rs_renderer::parse_mermaid_strict(body).unwrap_err();
        assert_eq!(said(&err, body), (Some(0), err.to_string()));
    }
}
