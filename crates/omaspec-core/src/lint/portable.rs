//! Mermaid flowcharts that Omaspec draws and mermaid.js refuses: the same
//! file is a parse error on GitHub and wherever else mermaid.js draws it.
//! Each case has a way of writing it that both read, which [`portable`]
//! puts in its place.
//!
//! The cases, as mermaid.js 12.3.0 reads them (when it learns one, the
//! case goes from here):
//!
//! - a `|` in a label without quotes: `B[Low | High]`
//!   (mermaid-js/mermaid#8750), read by both as `B["Low | High"]`;
//! - an emoji or another symbol in a subgraph's title without quotes:
//!   `subgraph 🔴 Breaks` (#8752), read by both as `subgraph "🔴 Breaks"`;
//! - a space before or after the `:::` of a class: `A[Start] :::hot`
//!   (#8753), read by both as `A[Start]:::hot`.

use std::ops::Range;

use super::{Finding, Row, Severity, mermaid_rows};
use crate::text::re;

/// What mermaid.js stops at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Why {
    Pipe,
    Title,
    Class,
}

/// A piece of a line and what both read in its place.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Edit {
    at: Range<usize>,
    with: String,
    why: Why,
}

/// The shapes of a box, longest first: what opens its label and what
/// closes it.
const SHAPES: &[(&str, &str)] = &[
    ("(((", ")))"),
    ("([", "])"),
    ("[[", "]]"),
    ("[(", ")]"),
    ("((", "))"),
    ("{{", "}}"),
    ("[/", "/]"),
    ("[\\", "\\]"),
    ("[/", "\\]"),
    ("[\\", "/]"),
    ("[", "]"),
    ("(", ")"),
    ("{", "}"),
];

/// A hint on each line of a flowchart that mermaid.js would not read.
pub(super) fn findings(rows: &[Row], findings: &mut Vec<Finding>) {
    for row in rows {
        let edits = edits(row.text);
        for why in [Why::Pipe, Why::Title, Why::Class] {
            let Some(first) = edits.iter().find(|e| e.why == why) else {
                continue;
            };
            let what = match why {
                Why::Pipe => "a `|` in a label without quotes",
                Why::Title => "a symbol in a subgraph's title without quotes",
                Why::Class => "a space beside `:::`",
            };
            let fixed = match why {
                // The whole label is short enough to show; a line is not.
                Why::Pipe | Why::Title => first.with.clone(),
                Why::Class => rewritten(row.text, &edits),
            };
            findings.push(Finding::new(
                row.row,
                Severity::Info,
                format!(
                    "Omaspec draws this, mermaid.js (GitHub) stops at {what}. \
                     Both read `{fixed}`: Rewrite for mermaid.js"
                ),
            ));
        }
    }
}

/// `code` with every such line written the way mermaid.js reads it too;
/// anything but a Mermaid flowchart comes back as it is.
pub fn portable(code: &str) -> String {
    if !is_flowchart(code) {
        return code.to_owned();
    }
    let lines: Vec<&str> = code.split('\n').collect();
    let mut out: Vec<String> = lines.iter().map(|l| (*l).to_owned()).collect();
    for row in mermaid_rows(&lines) {
        let edits = edits(row.text);
        if edits.is_empty() {
            continue;
        }
        let raw = lines[row.row];
        let lead = raw.len() - raw.trim_start().len();
        let tail = &raw[lead + row.text.len()..];
        out[row.row] = format!("{}{}{tail}", &raw[..lead], rewritten(row.text, &edits));
    }
    out.join("\n")
}

/// Whether `code` is a Mermaid flowchart, by its header.
pub(super) fn is_flowchart(code: &str) -> bool {
    crate::classify(code, crate::Language::Mermaid).name == "Flowchart"
}

/// `text` with its `edits`, which lie in order and apart.
fn rewritten(text: &str, edits: &[Edit]) -> String {
    let mut out = String::with_capacity(text.len() + 2 * edits.len());
    let mut from = 0;
    for edit in edits {
        out.push_str(&text[from..edit.at.start]);
        out.push_str(&edit.with);
        from = edit.at.end;
    }
    out.push_str(&text[from..]);
    out
}

/// `label` in quotes, a quote in it as mermaid.js writes one.
fn quoted(label: &str) -> String {
    format!("\"{}\"", label.trim().replace('"', "#quot;"))
}

/// What mermaid.js would not read on one line of a flowchart, in order.
/// A line that does not read here either (a label left open) gives none.
fn edits(text: &str) -> Vec<Edit> {
    if let Some(c) = re!(r"^subgraph[ \t]+(.+)$").captures(text) {
        let title = c.get(1).expect("the title");
        let said = title.as_str().trim_end();
        // In quotes and after an id (`subgraph id [title]`) mermaid.js
        // reads a title as text. Letters of any script are fine bare.
        let bare = !said.starts_with('"') && !said.contains('[');
        if bare && said.chars().any(|c| !c.is_ascii() && !c.is_alphanumeric()) {
            return vec![Edit {
                at: title.start()..title.start() + said.len(),
                with: quoted(said),
                why: Why::Title,
            }];
        }
        return Vec::new();
    }
    if re!(r"^(classDef|class|style|linkStyle|click|direction|end|accTitle|accDescr)\b")
        .is_match(text)
    {
        return Vec::new();
    }
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => match closing_quote(bytes, i) {
                Some(end) => i = end + 1,
                None => return Vec::new(),
            },
            // The label of a line: `A -->|text| B`.
            b'|' => match bytes[i + 1..].iter().position(|&b| b == b'|') {
                Some(len) => i += len + 2,
                None => return Vec::new(),
            },
            b'[' | b'(' | b'{' => {
                let Some((end, pipe)) = closing(bytes, i) else {
                    return Vec::new();
                };
                // `A@{ shape: … }` is a list of settings, not a label.
                let settings = bytes[i] == b'{' && i > 0 && bytes[i - 1] == b'@';
                if pipe
                    && !settings
                    && let Some(edit) = label(text, i, end)
                {
                    out.push(edit);
                }
                i = end + 1;
            }
            b':' if text[i..].starts_with(":::") => {
                let before = text[..i].trim_end().len();
                if before > 0 && before < i {
                    out.push(Edit {
                        at: before..i,
                        with: String::new(),
                        why: Why::Class,
                    });
                }
                let name = i + 3;
                let after = name + (text[name..].len() - text[name..].trim_start().len());
                if after > name && after < text.len() {
                    out.push(Edit {
                        at: name..after,
                        with: String::new(),
                        why: Why::Class,
                    });
                }
                i = after;
            }
            _ => i += 1,
        }
    }
    out
}

/// The quote that closes the one at `open`.
fn closing_quote(bytes: &[u8], open: usize) -> Option<usize> {
    bytes[open + 1..]
        .iter()
        .position(|&b| b == b'"')
        .map(|len| open + 1 + len)
}

/// The bracket that closes the one at `open`, and whether a `|` outside
/// quotes lies between them.
fn closing(bytes: &[u8], open: usize) -> Option<(usize, bool)> {
    let mut depth = 0usize;
    let mut pipe = false;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => i = closing_quote(bytes, i)?,
            b'[' | b'(' | b'{' => depth += 1,
            b']' | b')' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some((i, pipe));
                }
            }
            b'|' => pipe = true,
            _ => {}
        }
        i += 1;
    }
    None
}

/// The label of the box whose brackets run from `open` to `close`, in
/// quotes; none when it stands in quotes already.
fn label(text: &str, open: usize, close: usize) -> Option<Edit> {
    let span = &text[open..=close];
    let (opens, closes) = SHAPES.iter().find(|(opens, closes)| {
        span.len() >= opens.len() + closes.len()
            && span.starts_with(opens)
            && span.ends_with(closes)
    })?;
    let at = open + opens.len()..close + 1 - closes.len();
    let inner = &text[at.clone()];
    if inner.trim().starts_with('"') {
        return None;
    }
    Some(Edit {
        at,
        with: quoted(inner),
        why: Why::Pipe,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Language;
    use crate::lint::lint;

    fn said(code: &str) -> Vec<(usize, String)> {
        lint(code, Language::Mermaid)
            .into_iter()
            .filter(|f| f.message.contains("mermaid.js"))
            .map(|f| {
                assert_eq!(f.severity, Severity::Info);
                (f.line, f.message)
            })
            .collect()
    }

    #[test]
    fn a_pipe_in_a_label_goes_into_quotes_in_every_shape() {
        for (written, read) in [
            ("B[Low | High]", "B[\"Low | High\"]"),
            ("B(Low | High)", "B(\"Low | High\")"),
            ("B{Low | High}", "B{\"Low | High\"}"),
            ("B([Low | High])", "B([\"Low | High\"])"),
            ("B[[Low | High]]", "B[[\"Low | High\"]]"),
            ("B((a|b))", "B((\"a|b\"))"),
            ("B[/a | b/]", "B[/\"a | b\"/]"),
            ("B[(x) | y]", "B[\"(x) | y\"]"),
            ("B[say \"hi\" | x]", "B[\"say #quot;hi#quot; | x\"]"),
        ] {
            let code = format!("flowchart TD\n  A --> {written}\n");
            assert_eq!(
                portable(&code),
                format!("flowchart TD\n  A --> {read}\n"),
                "{written}"
            );
            let hints = said(&code);
            assert_eq!(hints.len(), 1, "{written}");
            assert_eq!(hints[0].0, 1);
            assert!(hints[0].1.contains("a `|` in a label"), "{}", hints[0].1);
        }
    }

    #[test]
    fn what_mermaid_js_reads_stays_as_it_is() {
        for line in [
            "A -->|yes | no| B",
            "A -->|a| B -->|b| C",
            "A[\"Low | High\"] --> B",
            "A[Start]:::hot --> B",
            "A@{ shape: rect, label: \"a | b\" }",
            "subgraph Two Words",
            "subgraph Café",
            "subgraph \"🔴 Breaks\"",
            "subgraph one [🔴 Breaks]",
            "classDef hot fill:#f96",
            "class A,B hot",
            "A[open | label",
            "click A \"https://example.org/a|b\"",
        ] {
            let code = format!("flowchart TD\n{line}\nend\n");
            assert_eq!(portable(&code), code, "{line}");
            assert_eq!(said(&code), [], "{line}");
        }
    }

    #[test]
    fn a_symbol_in_a_subgraph_s_title_goes_into_quotes() {
        let code = "flowchart TD\nsubgraph 🔴 Breaks\n  A --> B\nend\nsubgraph Done ✅  \nend\n";
        assert_eq!(
            portable(code),
            "flowchart TD\nsubgraph \"🔴 Breaks\"\n  A --> B\nend\nsubgraph \"Done ✅\"  \nend\n"
        );
        let hints = said(code);
        assert_eq!(hints.len(), 2);
        assert!(hints[0].1.contains("subgraph's title"), "{}", hints[0].1);
        assert!(hints[0].1.contains("`\"🔴 Breaks\"`"), "{}", hints[0].1);
    }

    #[test]
    fn a_space_beside_the_class_goes() {
        let code = "flowchart TD\n  A([Start]) :::auto --> B \t:::auto\n  C::: auto\n  classDef auto fill:#dff\n";
        assert_eq!(
            portable(code),
            "flowchart TD\n  A([Start]):::auto --> B:::auto\n  C:::auto\n  classDef auto fill:#dff\n"
        );
        let hints = said(code);
        assert_eq!(hints.len(), 2, "one for a line: {hints:?}");
        assert!(
            hints[0].1.contains("`A([Start]):::auto --> B:::auto`"),
            "{}",
            hints[0].1
        );
    }

    #[test]
    fn every_case_on_one_line_and_the_line_ends_stay() {
        let code = "graph LR\r\n\tA[a|b] :::x --> B{c|d}\r\n";
        assert_eq!(
            portable(code),
            "graph LR\r\n\tA[\"a|b\"]:::x --> B{\"c|d\"}\r\n"
        );
        assert_eq!(said(code).len(), 2);
        // Written the portable way, nothing is left to say.
        assert_eq!(said(&portable(code)), []);
    }

    #[test]
    fn only_flowcharts_are_looked_at() {
        for code in [
            "sequenceDiagram\n  A->>B: a | b :::c\n",
            "stateDiagram-v2\n  A --> B : x [a|b]\n",
            "@startuml\nA -> B : [a|b] :::c\n@enduml\n",
        ] {
            assert_eq!(portable(code), code);
            assert_eq!(said(code), [], "{code}");
        }
    }
}
