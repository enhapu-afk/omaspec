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
//! statement for each link. Every line keeps its number; only mmdr reads
//! the copy.

use std::borrow::Cow;

use mermaid_rs_renderer::parse_mermaid_strict;

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
    split_chains(text).or(quoted).or(titled)
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

/// Where the statements of `line` stand: apart at each `;`, up to a
/// comment. A `;` or `%%` in quotes or brackets is words.
fn statements(line: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let (mut depth, mut quote) = (0usize, false);
    let mut start = 0;
    let mut end = line.trim_end_matches(['\n', '\r']).len();
    for (at, ch) in line[..end].char_indices() {
        if quote {
            quote = ch != '"';
        } else if ch == '"' {
            quote = true;
        } else if matches!(ch, '[' | '(' | '{') {
            depth += 1;
        } else if matches!(ch, ']' | ')' | '}') {
            depth = depth.saturating_sub(1);
        } else if depth == 0 && ch == ';' {
            found.push((start, at));
            start = at + 1;
        } else if depth == 0 && line[at..].starts_with("%%") {
            end = at;
            break;
        }
    }
    found.push((start, end));
    found
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

/// A link of a statement: where it starts and ends, its label with it.
struct Link {
    start: usize,
    end: usize,
    labelled: bool,
}

/// The links of the statement `text`, outside quotes and boxes, each with
/// its label: `-->|yes|`, or `-- yes -->` from its first dashes to its
/// arrow (`-.yes.->` as mmdr reads it, without spaces).
fn links(text: &str) -> Vec<Link> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let byte = |index: usize| chars.get(index).map_or(text.len(), |&(at, _)| at);
    let mut found: Vec<Link> = Vec::new();
    // The opening dashes of a link with words, `-- yes -->`: where they
    // start and end, and whether a space follows them.
    let mut open: Option<(usize, usize, bool)> = None;
    let (mut depth, mut quote) = (0usize, false);
    let mut index = 0;
    while index < chars.len() {
        let (_, ch) = chars[index];
        if quote {
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
        } else if depth == 0
            && let Some((len, opens)) = link_at(&chars, index)
        {
            let mut end = index + len;
            let start = byte(index);
            if let Some((first, last, spaced)) = open {
                if opens {
                    // Dashes among the words.
                    index = end;
                    continue;
                }
                open = None;
                // The arrow of the words' link: the words were its label.
                if spaced || ch == '.' {
                    found.push(Link {
                        start: first,
                        end: byte(end),
                        labelled: true,
                    });
                    index = end;
                    continue;
                }
                // `-.b --> c`: the dots a link of their own, `b` its end.
                found.push(Link {
                    start: first,
                    end: byte(last),
                    labelled: false,
                });
            }
            let spaced = chars.get(end).is_some_and(|&(_, ch)| ch.is_whitespace());
            if opens && (spaced || text[start..byte(end)].ends_with("-.")) {
                open = Some((start, end, spaced));
                index = end;
                continue;
            }
            // A label in bars after the arrow, `-->|yes|` or `--> |yes|`.
            let mut next = end;
            while chars
                .get(next)
                .is_some_and(|&(_, ch)| ch == ' ' || ch == '\t')
            {
                next += 1;
            }
            let mut labelled = false;
            if chars.get(next).map(|&(_, ch)| ch) == Some('|')
                && let Some(close) = chars[next + 1..].iter().position(|&(_, ch)| ch == '|')
            {
                end = next + close + 2;
                labelled = true;
            }
            found.push(Link {
                start,
                end: byte(end),
                labelled,
            });
            index = end;
            continue;
        }
        index += 1;
    }
    // Opening dashes no arrow followed are a link without words.
    if let Some((start, last, _)) = open {
        let at = found.partition_point(|link| link.start < start);
        found.insert(
            at,
            Link {
                start,
                end: byte(last),
                labelled: false,
            },
        );
    }
    found
}

/// How many characters the link at `index` runs, and whether it is the
/// opening `--`, `==` or `-.` of a link with words; `None` when no link
/// starts there. A circle or a cross starts one only where no name runs
/// into it (`A o--o B`), so `box-->c` is the box `box`; a link has two
/// dashes, or one with a dot or an arrow (`-.`, `.->`, `->`).
fn link_at(chars: &[(usize, char)], index: usize) -> Option<(usize, bool)> {
    let at = |k: usize| chars.get(k).map(|&(_, ch)| ch);
    let mut k = index;
    let mut marked = false;
    match at(k)? {
        '<' => {
            k += 1;
            marked = true;
        }
        'o' | 'x' => {
            let named =
                index > 0 && at(index - 1).is_some_and(|ch| ch.is_alphanumeric() || ch == '_');
            if named || !matches!(at(k + 1), Some('-' | '=' | '.')) {
                return None;
            }
            k += 1;
        }
        '-' | '=' | '.' => {}
        _ => return None,
    }
    let body = k;
    while matches!(at(k), Some('-' | '=' | '.')) {
        k += 1;
    }
    let run: String = chars[body..k].iter().map(|&(_, ch)| ch).collect();
    let lines = run.chars().filter(|ch| matches!(ch, '-' | '=')).count();
    marked |= run.contains('.');
    let opens = matches!(run.as_str(), "--" | "==" | "-.");
    match at(k) {
        Some('>') => {
            k += 1;
            marked = true;
        }
        Some('o' | 'x') if k > body => k += 1,
        _ => {}
    }
    let opens = opens && k == body + run.len();
    (lines >= 2 || (lines == 1 && marked)).then_some((k - index, opens))
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
            // A quote mark after the last `;` hides none.
            ("a -->|x| b --> c[it's]\n", "a -->|x| b; b --> c[it's]\n"),
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
            // A `'` or a `\` before a `;` would hide it from mmdr, and a
            // circle's dashes after a label would leave a link without
            // its end: mmdr's reading, as it was.
            "flowchart LR\na -->|don't| b --> c\n",
            "flowchart LR\na -->|x| b --> c[it's] --> d\n",
            "flowchart LR\na -->|x| B[C:\\] --> c\n",
            "flowchart LR\na -->|x|o-->b\n",
        ] {
            assert!(matches!(readable(kept), Cow::Borrowed(_)), "{kept}");
        }
    }

    #[test]
    fn the_lines_keep_their_numbers() {
        let body = "flowchart LR\n  title --> A>a-b]\n\n  A --> C\n";
        let read = readable(body);
        assert_eq!(read.lines().count(), body.lines().count());
        assert_eq!(read.lines().nth(1), Some("  title\t--> A>\"a-b\"]"));
    }
}
