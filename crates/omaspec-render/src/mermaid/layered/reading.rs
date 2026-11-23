//! A flowchart as mmdr is given it to read.
//!
//! mmdr skips a line that starts with `title `, a title in other kinds,
//! though in a flowchart `title --> B` links a node named `title`: the
//! line was lost without a word. And it takes a `-`, `=` or `&` in the
//! text of an asymmetric box, `A>svc-30]`, for a link, and draws two
//! boxes named `A>svc` and `30]`. And it splits a chain into its links
//! only where none is labelled: after `A -->|yes| B --> C` it took all
//! that follows the label for one node, `B`, and `C` was lost. These are
//! written here so mmdr reads them as Mermaid does: the space after
//! `title` as a tab, the box's text in quotes, a chain with a label as a
//! statement for each link. mmdr also takes a `'` for a quote mark,
//! where Mermaid takes it for a letter: `a[it's] --> b` lost `a`, and
//! after `a -->|don't| b --> c` the rest of the chain (REQ-102). In a
//! box's text, in quotes and in a link's label it is written as a letter
//! mmdr has no use for, and put back in what mmdr read ([`read`]).
//! Every line keeps its number; only mmdr reads the copy.

use std::borrow::Cow;

use mermaid_rs_renderer::{Graph, ParseError, ParseOutput, parse_mermaid_strict};
use omaspec_core::mermaid_flow::{links, statements};

/// `body` with the lines mmdr misreads written so it reads them; `body`
/// itself when none is.
pub(in crate::mermaid) fn readable(body: &str) -> Cow<'_, str> {
    let mut out = String::new();
    let mut changed = false;
    let mut front = false;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed == "---" {
            front = !front;
        }
        let read = if front || trimmed == "---" || trimmed.starts_with("%%") {
            None
        } else {
            read_line(line)
        };
        match read {
            Some(read) => {
                changed = true;
                out.push_str(&read);
            }
            None => out.push_str(line),
        }
    }
    if changed {
        Cow::Owned(out)
    } else {
        Cow::Borrowed(body)
    }
}

/// The line as mmdr is to read it, or `None` when it reads it right.
fn read_line(line: &str) -> Option<String> {
    let titled = title_node(line);
    let text = titled.as_deref().unwrap_or(line);
    let quoted = quote_asymmetric(text);
    let text = quoted.as_deref().unwrap_or(text);
    let masked = mask_apostrophes(text);
    let text = masked.as_deref().unwrap_or(text);
    split_chains(text).or(masked).or(quoted).or(titled)
}

/// What stands for a `'` in mmdr's copy: a character of the private use
/// area, which no text writes.
const APOSTROPHE: char = '\u{E027}';

/// Words that start a statement about parts: their quotes are CSS's or a
/// call's (`style a font-family:'Mono'`), and stay.
const PARTS: &[&str] = &["classDef", "style", "linkStyle", "click"];

/// Each `'` on `line` in a box's text, in quotes or in a link's label,
/// written as [`APOSTROPHE`]; `None` where there is none. One in a name
/// stays: a name is not put back.
fn mask_apostrophes(line: &str) -> Option<String> {
    if !line.contains('\'') {
        return None;
    }
    let mut at = Vec::new();
    for (start, end) in statements(line) {
        let text = &line[start..end];
        let word = text
            .trim_start()
            .split(|c: char| c.is_whitespace() || c == ':')
            .next()
            .unwrap_or("");
        if PARTS.contains(&word) {
            continue;
        }
        let labels = links(text);
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let (mut depth, mut quote) = (0usize, false);
        for (index, &(byte, ch)) in chars.iter().enumerate() {
            if ch == '\'' {
                let labelled = labels
                    .iter()
                    .any(|link| (link.start..link.end).contains(&byte));
                if depth > 0 || quote || labelled {
                    at.push(start + byte);
                }
            } else if quote {
                quote = ch != '"';
            } else if ch == '"' {
                quote = true;
            } else if matches!(ch, '[' | '(' | '{') {
                depth += 1;
            } else if matches!(ch, ']' | ')' | '}') {
                depth = depth.saturating_sub(1);
            } else if ch == '>'
                && depth == 0
                && index > 0
                && (chars[index - 1].1.is_alphanumeric() || chars[index - 1].1 == '_')
            {
                // An asymmetric box, `id>text]`, open to its `]`.
                depth += 1;
            }
        }
    }
    if at.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(line.len() + 2 * at.len());
    let mut from = 0;
    for byte in at {
        out.push_str(&line[from..byte]);
        out.push(APOSTROPHE);
        from = byte + 1;
    }
    out.push_str(&line[from..]);
    Some(out)
}

/// The `'` written as [`APOSTROPHE`] put back in the words of what mmdr
/// read: the boxes' text, the links' labels and the subgraphs' titles.
pub(in crate::mermaid) fn restore(graph: &mut Graph) {
    let back = |text: &mut String| {
        if text.contains(APOSTROPHE) {
            *text = text.replace(APOSTROPHE, "'");
        }
    };
    for node in graph.nodes.values_mut() {
        back(&mut node.label);
    }
    for edge in &mut graph.edges {
        for label in [&mut edge.label, &mut edge.start_label, &mut edge.end_label]
            .into_iter()
            .flatten()
        {
            back(label);
        }
    }
    for subgraph in &mut graph.subgraphs {
        back(&mut subgraph.label);
    }
}

/// `words` from mmdr, a refusal's, with each `'` put back.
pub(in crate::mermaid) fn unmasked(words: &str) -> String {
    words.replace(APOSTROPHE, "'")
}

/// The flowchart `body` as mmdr reads the copy written for it
/// ([`readable`]), its words as written.
pub(in crate::mermaid) fn read(body: &str) -> Result<ParseOutput, ParseError> {
    let mut parsed = parse_mermaid_strict(&readable(body))?;
    restore(&mut parsed.graph);
    Ok(parsed)
}

/// `title --> B` with a tab after `title`, which mmdr does not skip. A
/// line where words follow `title` is a title as mmdr has it, and stays.
fn title_node(line: &str) -> Option<String> {
    let indent = line.len() - line.trim_start().len();
    let rest = line[indent..].strip_prefix("title ")?;
    let next = rest.trim_start().chars().next()?;
    if !matches!(next, '-' | '=' | '.' | '<' | '~' | '&' | ':' | '@') {
        return None;
    }
    let at = indent + "title".len();
    Some(format!("{}\t{}", &line[..at], &line[at + 1..]))
}

/// Each asymmetric box `id>text]` whose text has a `-`, `=` or `&`, its
/// text put in quotes; `None` when there is none. A `>` that ends an
/// arrow (`-->`, `==>`) follows a `-` or `=`, not a name; text in quotes,
/// in a box or in a link's `|label|` is skipped.
fn quote_asymmetric(line: &str) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len() + 4);
    let mut changed = false;
    let (mut depth, mut quote, mut label) = (0usize, false, false);
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if quote {
            quote = ch != '"';
        } else if label {
            label = ch != '|';
        } else if ch == '"' {
            quote = true;
        } else if ch == '%' && chars.get(index + 1) == Some(&'%') {
            // A comment to the end of the line.
            out.extend(&chars[index..]);
            break;
        } else if ch == '|' && depth == 0 {
            label = true;
        } else if matches!(ch, '[' | '(' | '{') {
            depth += 1;
        } else if matches!(ch, ']' | ')' | '}') {
            depth = depth.saturating_sub(1);
        } else if ch == '>'
            && depth == 0
            && index > 0
            && (chars[index - 1].is_alphanumeric() || chars[index - 1] == '_')
            && let Some(len) = asymmetric_text(&chars[index + 1..])
        {
            let text: String = chars[index + 1..index + 1 + len].iter().collect();
            out.push('>');
            if text.contains(['-', '=', '&']) {
                out.push('"');
                out.push_str(&text);
                out.push('"');
                changed = true;
            } else {
                out.push_str(&text);
            }
            out.push(']');
            index += len + 2;
            continue;
        }
        out.push(ch);
        index += 1;
    }
    changed.then_some(out)
}

/// How long the text of an asymmetric box is, up to its `]`; `None` when
/// no `]` closes it, or the text is quoted, or holds a box or a label.
fn asymmetric_text(rest: &[char]) -> Option<usize> {
    let len = rest.iter().position(|&ch| ch == ']')?;
    let text = &rest[..len];
    (!text.is_empty() && !text.iter().any(|ch| matches!(ch, '"' | '[' | '|'))).then_some(len)
}

/// Each chain on `line` with a labelled link written as one statement
/// for each of its links, `A -->|yes| B --> C` as `A -->|yes| B; B --> C`;
/// `None` when no chain on the line has a label. The statements are the
/// line's own words, so each keeps its spelling, and mmdr reads every
/// link as it reads one alone.
fn split_chains(line: &str) -> Option<String> {
    let mut out = String::with_capacity(line.len() + 8);
    let mut from = 0;
    for (start, end) in statements(line) {
        if let Some(split) = split_chain(&line[start..end]) {
            out.push_str(&line[from..start]);
            out.push_str(&split);
            from = end;
        }
    }
    (from > 0).then(|| {
        out.push_str(&line[from..]);
        out
    })
}

/// The statement `text` as one statement for each link, when it is a
/// chain with a labelled link; `None` otherwise. Each statement runs from
/// one end through its link to the next end, as written.
fn split_chain(text: &str) -> Option<String> {
    let lead = text.trim_start();
    let word = lead
        .split(|c: char| c.is_whitespace() || c == ':')
        .next()
        .unwrap_or("");
    // Words mmdr takes a statement about parts by, as it does: some as a
    // start, some as the first word.
    let starts = ["classDef", "linkStyle", "click", "accTitle", "accDescr"];
    let words = ["subgraph", "class", "style"];
    // A title as mmdr has it, a statement about parts, and one with an
    // invisible link (repaired after mmdr's reading) stay.
    if lead.starts_with("title ")
        || starts.iter().any(|start| lead.starts_with(start))
        || words.contains(&word)
        || text.contains("~~~")
    {
        return None;
    }
    let links = links(text);
    if links.len() < 2 || !links.iter().any(|link| link.labelled) {
        return None;
    }
    let mut ends = Vec::with_capacity(links.len() + 1);
    let mut from = 0;
    for link in &links {
        ends.push((from, link.start));
        from = link.end;
    }
    ends.push((from, text.len()));
    // Where an end's words start and stop; `None` for an end without any.
    let words = |(start, end): (usize, usize)| {
        let part = &text[start..end];
        let trimmed = part.trim_start();
        (!trimmed.trim_end().is_empty()).then(|| {
            let first = start + part.len() - trimmed.len();
            (first, first + trimmed.trim_end().len())
        })
    };
    let ends: Vec<(usize, usize)> = ends.into_iter().map(words).collect::<Option<_>>()?;
    let pieces: Vec<&str> = (0..links.len())
        .map(|index| &text[ends[index].0..ends[index + 1].1])
        .collect();
    let split = pieces.join("; ");
    reads_apart(&pieces, &split).then(|| {
        format!(
            "{}{split}{}",
            &text[..ends[0].0],
            &text[ends[links.len()].1..]
        )
    })
}

/// Whether mmdr reads `split`, the statements of `pieces` on one line, as
/// it reads each of them alone, and each as a link. It takes a `'` for a
/// quote mark even in a label (`-->|don't|`), and a `\` for an escape, so
/// a `;` after one is words to it; and a statement it cannot read alone
/// would refuse the whole diagram. Such a chain is left as it was.
fn reads_apart(pieces: &[&str], split: &str) -> bool {
    let edges = |text: &str| {
        crate::caught::caught(|| parse_mermaid_strict(&format!("flowchart TD\n{text}")))
            .ok()?
            .ok()
            .map(|read| {
                read.graph
                    .edges
                    .into_iter()
                    .map(|edge| (edge.from, edge.to, edge.label))
                    .collect::<Vec<_>>()
            })
    };
    let mut apart = Vec::new();
    for piece in pieces {
        match edges(piece) {
            Some(read) if !read.is_empty() => apart.extend(read),
            _ => return false,
        }
    }
    edges(split) == Some(apart)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_named_title_is_read() {
        assert_eq!(
            readable("flowchart LR\n  title --> done[End]\n"),
            "flowchart LR\n  title\t--> done[End]\n"
        );
        assert_eq!(
            readable("flowchart LR\ntitle & other --> x\n"),
            "flowchart LR\ntitle\t& other --> x\n"
        );
        // A title as mmdr reads one, a box named title, and the front
        // matter's title stay as they are.
        for kept in [
            "flowchart LR\ntitle Orders\nA --> B\n",
            "flowchart LR\ntitle[Title page] --> B\n",
            "---\ntitle: Orders\n---\nflowchart LR\nA --> B\n",
        ] {
            assert!(matches!(readable(kept), Cow::Borrowed(_)), "{kept}");
        }
    }

    #[test]
    fn an_asymmetric_box_keeps_its_dash_in_quotes() {
        assert_eq!(
            readable("flowchart LR\n  A>svc-30] --> B[Done]\n"),
            "flowchart LR\n  A>\"svc-30\"] --> B[Done]\n"
        );
        assert_eq!(
            readable("flowchart LR\nX --> A>a=b] & C>x&y]\n"),
            "flowchart LR\nX --> A>\"a=b\"] & C>\"x&y\"]\n"
        );
        for kept in [
            // Arrows, plain text, quoted text, a label, a box's text.
            "flowchart LR\nA-->B\nA==>B\nA-.->B\nA -- go --> B\n",
            "flowchart LR\nA>v1.2] --> B>a b]\n",
            "flowchart LR\nA>\"svc-30\"] --> B\n",
            "flowchart LR\nA -->|a>b-c]| B\n",
            "flowchart LR\nA[x>y-z] --> B\n",
            "flowchart LR\nA --> B %% C>d-e]\n",
            "%% A>d-e]\nflowchart LR\nA --> B\n",
        ] {
            assert!(matches!(readable(kept), Cow::Borrowed(_)), "{kept}");
        }
    }

    #[test]
    fn a_chain_with_a_label_is_read_link_by_link() {
        for (written, read) in [
            (
                "  a -->|yes| b --> c --> d\n",
                "  a -->|yes| b; b --> c; c --> d\n",
            ),
            ("a -- yes --> b --> c\n", "a -- yes --> b; b --> c\n"),
            ("a --> b -- no --> c\n", "a --> b; b -- no --> c\n"),
            ("a --> |yes| b --> c", "a --> |yes| b; b --> c"),
            ("a -. maybe .-> b ==> c\n", "a -. maybe .-> b; b ==> c\n"),
            ("a == sure ==> b --- c\n", "a == sure ==> b; b --- c\n"),
            (
                "A[Start] & B -->|go| C{Ok?}:::warm --o D(Done)\n",
                "A[Start] & B -->|go| C{Ok?}:::warm; C{Ok?}:::warm --o D(Done)\n",
            ),
            (
                "graph TD; a -->|x| b --> c\n",
                "graph TD; a -->|x| b; b --> c\n",
            ),
            (
                "a -->|x| b --> c %% then c\n",
                "a -->|x| b; b --> c %% then c\n",
            ),
            (
                "a -- \"is it -- or not\" --> b --> c\n",
                "a -- \"is it -- or not\" --> b; b --> c\n",
            ),
            ("a -- re-try --> b --> c\n", "a -- re-try --> b; b --> c\n"),
            ("box -->|in| ox o--o fox\n", "box -->|in| ox; ox o--o fox\n"),
            (
                "A>go] -->|on| B[x --> y] <--> C\n",
                "A>go] -->|on| B[x --> y]; B[x --> y] <--> C\n",
            ),
            ("a -.yes.-> b --> c\n", "a -.yes.-> b; b --> c\n"),
            // A quote mark in a label or a box is a letter mmdr reads.
            (
                "a -->|x| b --> c[it's]\n",
                "a -->|x| b; b --> c[it\u{E027}s]\n",
            ),
            ("a -->|don't| b --> c\n", "a -->|don\u{E027}t| b; b --> c\n"),
            (
                "a -->|x| b --> c[it's] --> d\n",
                "a -->|x| b; b --> c[it\u{E027}s]; c[it\u{E027}s] --> d\n",
            ),
        ] {
            assert_eq!(
                readable(&format!("flowchart LR\n{written}")),
                format!("flowchart LR\n{read}"),
                "{written}"
            );
        }
        for kept in [
            // Chains without labels, and labelled links alone.
            "flowchart LR\na --> b --> c\na -->|yes| b\na -- yes --> b\n",
            // Words in a label, a box or quotes; not a link.
            "flowchart LR\na -->|x --> y| b\na[p --> q --> r] -->|s| b\n",
            "flowchart LR\nmy-node -->|x| other-node\n",
            // Statements about parts, a title, an invisible link.
            "flowchart LR\nlinkStyle 0 stroke-width:2px\nstyle a stroke-dasharray: 5 5\n",
            "flowchart LR\ntitle Go -->|x| b --> c\na ~~~ b -->|x| c --> d\n",
            "flowchart LR\n%% a -->|x| b --> c\n",
            // A `\` before a `;` would hide it from mmdr, and a circle's
            // dashes after a label would leave a link without its end:
            // mmdr's reading, as it was.
            "flowchart LR\na -->|x| B[C:\\] --> c\n",
            "flowchart LR\na -->|x|o-->b\n",
        ] {
            assert!(matches!(readable(kept), Cow::Borrowed(_)), "{kept}");
        }
    }

    /// A `'` in a box, in quotes or in a label reaches mmdr as a letter
    /// and comes back from it as written (REQ-102): `a[it's] --> b` lost
    /// `a`, and a chain after `-->|don't|` lost its rest.
    #[test]
    fn a_quote_mark_is_a_letter() {
        for (body, nodes, labels) in [
            (
                "flowchart LR\n  a[it's] --> b\n  b --> c\n",
                vec!["it's", "b", "c"],
                vec![],
            ),
            (
                "flowchart LR\n  a -->|don't| b --> c\n",
                vec!["a", "b", "c"],
                vec!["don't"],
            ),
            (
                "flowchart LR\n  a[it's] -->|x| b --> c\n",
                vec!["it's", "b", "c"],
                vec!["x"],
            ),
            (
                "flowchart LR\n  a -- don't --> b --> c\n",
                vec!["a", "b", "c"],
                vec!["don't"],
            ),
            (
                "flowchart LR\n  a[\"it's\"] --> b((Bob's)) --> c>it's]\n",
                vec!["it's", "Bob's", "it's"],
                vec![],
            ),
            (
                "flowchart LR\n  subgraph s [Ann's]\n    a\n  end\n",
                vec!["a"],
                vec![],
            ),
        ] {
            let graph = read(body).unwrap().graph;
            let mut found: Vec<&str> = graph.nodes.values().map(|n| n.label.as_str()).collect();
            let mut wanted = nodes.clone();
            found.sort_unstable();
            wanted.sort_unstable();
            assert_eq!(found, wanted, "{body}");
            let said: Vec<&str> = graph
                .edges
                .iter()
                .filter_map(|e| e.label.as_deref())
                .collect();
            assert_eq!(said, labels, "{body}");
            if body.contains("subgraph") {
                assert_eq!(graph.subgraphs[0].label, "Ann's");
            }
        }
        // A statement about parts keeps its quotes.
        let style = "flowchart LR\n  style a font-family:'Mono'\n";
        assert!(matches!(readable(style), Cow::Borrowed(_)));
    }

    #[test]
    fn the_lines_keep_their_numbers() {
        let body = "flowchart LR\n  title --> A>a-b]\n\n  A --> C\n";
        let read = readable(body);
        assert_eq!(read.lines().count(), body.lines().count());
        assert_eq!(read.lines().nth(1), Some("  title\t--> A>\"a-b\"]"));
    }
}
