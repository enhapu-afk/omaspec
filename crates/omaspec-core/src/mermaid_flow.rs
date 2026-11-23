//! A Mermaid flowchart's statements and links as mermaid.js lexes them
//! (`flow.jison`): where the statements of a line stand, where their links
//! run, and the lines mermaid.js refuses that mmdr reads otherwise.
//!
//! A box's name is a run of letters without a space: after `D --> Order
//! parts` mermaid.js stops at `parts`, a parse error, and mmdr drew a box
//! `Order` and dropped the rest without a word (REQ-112). Render and lint
//! refuse such a line alike ([`wrong`]).

/// Where the statements of `line` stand: apart at each `;`, up to a
/// comment. A `;` or `%%` in quotes or brackets is words.
pub fn statements(line: &str) -> Vec<(usize, usize)> {
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

/// A link of a statement: where it starts and ends, its label with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    pub start: usize,
    pub end: usize,
    pub labelled: bool,
}

/// The links of the statement `text`, outside quotes and boxes, each with
/// its label: `-->|yes|`, or `-- yes -->` from its first dashes to its
/// arrow (`-.yes.->` as mmdr reads it, without spaces).
pub fn links(text: &str) -> Vec<Link> {
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
            && open.is_none()
            && index > 0
            && named(chars[index - 1].1)
        {
            // An asymmetric box, `id>text]`, open to its `]`; not in the
            // words of a link (`-- n>0 -->`).
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

/// Whether `ch` may end a box's name, so a `>` after it opens an
/// asymmetric box and an `o` or `x` after it is no link's end.
fn named(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// How many characters the link at `index` runs, and whether it is the
/// opening `--`, `==` or `-.` of a link with words; `None` when no link
/// starts there. A circle or a cross starts one only where no name runs
/// into it (`A o--o B`), so `box-->c` is the box `box`; a link has two
/// dashes, or one with a dot or an arrow (`-.`, `.->`, `->`). An invisible
/// link is three tildes or more, `~~~`.
fn link_at(chars: &[(usize, char)], index: usize) -> Option<(usize, bool)> {
    let at = |k: usize| chars.get(k).map(|&(_, ch)| ch);
    let mut k = index;
    let mut marked = false;
    match at(k)? {
        '~' => {
            while at(k) == Some('~') {
                k += 1;
            }
            return (k - index >= 3).then_some((k - index, false));
        }
        '<' => {
            k += 1;
            marked = true;
        }
        'o' | 'x' => {
            let after_name = index > 0 && at(index - 1).is_some_and(named);
            if after_name || !matches!(at(k + 1), Some('-' | '=' | '.')) {
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

/// Words that start a statement about parts, not about boxes: what
/// follows them is no box's name. `title` as mmdr has it, a title.
const KEYWORDS: &[&str] = &[
    "subgraph",
    "end",
    "direction",
    "classDef",
    "class",
    "style",
    "linkStyle",
    "click",
    "accTitle",
    "accDescr",
    "title",
];

/// Where a quote or a bracket stands open at the end of `line`, from
/// `open` at its start; a comment ends it. A link's label in bars
/// (`-->|n>0|`) is words, opening nothing.
fn carried(line: &str, open: (usize, bool)) -> (usize, bool) {
    let (mut depth, mut quote) = open;
    let mut label = false;
    let chars: Vec<char> = line.chars().collect();
    for (index, &ch) in chars.iter().enumerate() {
        if quote {
            quote = ch != '"';
        } else if label {
            label = ch != '|';
        } else if ch == '"' {
            quote = true;
        } else if ch == '|' && depth == 0 {
            label = true;
        } else if matches!(ch, '[' | '(' | '{') {
            depth += 1;
        } else if matches!(ch, ']' | ')' | '}') {
            depth = depth.saturating_sub(1);
        } else if ch == '>' && index > 0 && named(chars[index - 1]) && depth == 0 {
            depth += 1;
        } else if depth == 0 && ch == '%' && chars.get(index + 1) == Some(&'%') {
            break;
        }
    }
    (depth, quote)
}

/// A name of a box in an end: where it stands in the end, and whether
/// something parts it from the name before.
struct Name {
    start: usize,
    end: usize,
    parted: bool,
}

/// The names of boxes in `end`, the words between two links (or before
/// the first, after the last), outside quotes and brackets: each with
/// whether something parts it from the name before, an `&` or dashes the
/// lexer took for no link. A class (`:::hot`), shape data (`@{ … }`), a
/// box's text after a space and a link's id before its link (`e1@-->`)
/// belong to the name before them.
fn names(end: &str) -> Vec<Name> {
    let chars: Vec<(usize, char)> = end.char_indices().collect();
    let mut words = Vec::new();
    let (mut depth, mut quote) = (0usize, false);
    let mut start: Option<usize> = None;
    for (index, &(at, ch)) in chars.iter().enumerate() {
        if !quote && depth == 0 && ch.is_whitespace() {
            if let Some(from) = start.take() {
                words.push((from, at));
            }
            continue;
        }
        start.get_or_insert(at);
        if quote {
            quote = ch != '"';
        } else if ch == '"' {
            quote = true;
        } else if matches!(ch, '[' | '(' | '{') {
            depth += 1;
        } else if matches!(ch, ']' | ')' | '}') {
            depth = depth.saturating_sub(1);
        } else if ch == '>' && index > 0 && named(chars[index - 1].1) && depth == 0 {
            depth += 1;
        }
    }
    if let Some(from) = start {
        words.push((from, end.len()));
    }
    let mut found: Vec<Name> = Vec::new();
    let mut parted = true;
    for (from, to) in words {
        let word = &end[from..to];
        if word == "&"
            || word
                .chars()
                .all(|ch| matches!(ch, '~' | '-' | '=' | '.' | '<' | '>'))
        {
            parted = true;
        } else if word.starts_with(":::")
            || word.starts_with("@{")
            || word.starts_with(['[', '(', '{', '>'])
            || word.ends_with('@')
        {
            // Of the name before it.
            if let Some(last) = found.last_mut() {
                last.end = to;
            }
        } else {
            found.push(Name {
                start: from,
                end: to,
                parted,
            });
            parted = false;
        }
    }
    found
}

/// The first run of names in `statement` that stand side by side without
/// a link or an `&` between them, from its first name to its last, which
/// mermaid.js refuses: the run's text and its first name.
fn side_by_side(statement: &str) -> Option<(&str, &str)> {
    let lead = statement.trim_start();
    let word = lead
        .split(|c: char| c.is_whitespace() || c == ':' || c == ';')
        .next()
        .unwrap_or("");
    if lead.is_empty() || KEYWORDS.contains(&word) {
        return None;
    }
    let links = links(statement);
    let mut from = 0;
    let bounds = links
        .iter()
        .map(|link| link.start)
        .chain([statement.len()])
        .zip(links.iter().map(|link| link.end).chain([statement.len()]));
    for (to, next) in bounds {
        let end = &statement[from..to];
        let words = names(end);
        if let Some(first) = words.windows(2).position(|pair| !pair[1].parted) {
            let last = words[first + 1..]
                .iter()
                .take_while(|name| !name.parted)
                .last()?;
            let name = &words[first];
            return Some((&end[name.start..last.end], &end[name.start..name.end]));
        }
        from = next;
    }
    None
}

/// What is said of `run`, names side by side whose first is `name`: an
/// `&` written onto a name, a box's text after a space, or one name with
/// a space in it.
fn said(run: &str, name: &str) -> String {
    if run.contains('&') {
        format!(
            "`{run}`: an `&` stands apart from the names it joins (`A & B`), \
             and Mermaid refuses this line too"
        )
    } else if run.contains(['[', ']', '(', ')', '{', '}', '>', '"']) {
        format!(
            "`{run}`: a box's text follows its name without a space between, \
             and Mermaid refuses this line too"
        )
    } else {
        format!(
            "`{run}` stands where one name goes: a box's name has no space, and \
             Mermaid refuses this line too; its words go in brackets, as \
             `{name}[{run}]`"
        )
    }
}

/// The first line of the flowchart (or swimlane diagram) `body` that
/// mermaid.js refuses and mmdr would read otherwise, with what is said of
/// it: a box's name with a space (`D --> Order parts`). `None` for a text
/// of another kind. The statements start after the header (`flowchart
/// TD`), found and judged as [`crate::mermaid_head`] has it, or on it
/// where it names no kind and mmdr reads a flowchart's line; lines in a
/// directive over several lines, in comments, in `accDescr { … }` and
/// within quotes or brackets that run over several lines are words, not
/// statements.
pub fn wrong(body: &str) -> Option<(usize, String)> {
    use crate::mermaid_head::{Head, head_line, head_of, kind_of};
    let lines: Vec<&str> = body.split('\n').collect();
    let head = head_line(&lines)?;
    let headed = match kind_of(lines[head]) {
        Some("flowchart" | "graph" | "swimlane") => true,
        Some(_) => return None,
        None if head_of(lines[head]) == Head::Flowchart => false,
        None => return None,
    };
    let (mut directive, mut described) = (false, false);
    let mut open = (0usize, false);
    for (index, line) in lines.iter().enumerate().skip(head) {
        let trimmed = line.trim();
        if described {
            described = !trimmed.contains('}');
            continue;
        }
        if directive {
            directive = !trimmed.contains("}%%");
            continue;
        }
        if trimmed.starts_with("%%{") {
            directive = !trimmed.contains("}%%");
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("%%") {
            continue;
        }
        // A description over several lines: any words but a `}`.
        if trimmed
            .strip_prefix("accDescr")
            .is_some_and(|rest| rest.trim_start().starts_with('{'))
        {
            described = !trimmed.contains('}');
            continue;
        }
        let mut parts = statements(line).into_iter();
        if index == head && headed {
            parts.next();
        }
        let was = open;
        open = carried(line, was);
        if was != (0, false) || open != (0, false) {
            continue;
        }
        for (start, end) in parts {
            if let Some((run, name)) = side_by_side(&line[start..end]) {
                return Some((index, said(run, name)));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn said(body: &str) -> Option<usize> {
        wrong(body).map(|(line, _)| line)
    }

    #[test]
    fn a_name_with_a_space_is_refused_on_its_line() {
        assert_eq!(
            said("flowchart TD\n    A --> B\n    D --> Order parts\n"),
            Some(2)
        );
        assert_eq!(said("flowchart TD\n    Adjust again --> B\n"), Some(1));
        assert_eq!(said("graph LR\n    A --> B & Order parts\n"), Some(1));
        assert_eq!(said("flowchart TD\n    A --> B:Order parts\n"), Some(1));
        assert_eq!(
            said("flowchart TD\n    A[Start] --> Order parts --> C\n"),
            Some(1)
        );
        assert_eq!(said("flowchart TD\n    A --> B; C D\n"), Some(1));
        let (_, words) = wrong("flowchart TD\n    D --> Order parts\n").unwrap();
        assert!(words.contains("`Order parts`"), "{words}");
        assert!(words.contains("`Order[Order parts]`"), "{words}");
        // The whole run of words, and what else it is.
        let words = |body: &str| wrong(body).unwrap().1;
        assert!(
            words("flowchart TD\n    A --> Order many parts\n")
                .contains("`Order[Order many parts]`")
        );
        assert!(words("flowchart TD\n    A &B\n").contains("an `&` stands apart"));
        assert!(words("flowchart TD\n    A >some text]\n").contains("without a space between"));
        // A label in bars with a `>` opens no box (it hid the rest), a
        // head as mmdr reads it, a flowchart without one, and swimlanes.
        assert_eq!(
            said("flowchart TD\n    A -->|n>0| B\n    C --> Order parts\n"),
            Some(2)
        );
        assert_eq!(said("Flowchart TD\n    A --> Order parts\n"), Some(1));
        assert_eq!(said("A --> Order parts\n"), Some(0));
        assert_eq!(
            said("swimlane-beta LR\n  subgraph shop [Shop]\n    A --> Order parts\n  end\n"),
            Some(2)
        );
        // Other kinds are not flowcharts.
        assert_eq!(said("sequenceDiagram\n    Order parts\n"), None);
    }

    #[test]
    fn what_mermaid_reads_is_not_refused() {
        for body in [
            "flowchart TD\n    A[Order parts] --> B(Adjust again)\n",
            "flowchart TD\n    A -- Order parts --> B\n    A == Order parts ==> B\n",
            "flowchart TD\n    A -. Order parts .-> B\n    A -->|Order parts| B\n",
            "flowchart TD\n    A --> |Order parts| B\n    A --- |x y| B\n",
            "flowchart TD\n    A & B --> C & D\n    A&B --> C\n",
            "flowchart TD\n    A ~~~ B\n    A:::hot --> B\n",
            "flowchart TD\n    A ~~~B\n    C~~~ D\n    E~~~~F\n",
            "flowchart TD\n  accDescr {\n    1) Order parts\n    2) Ship them\n  }\n  A --> B\n",
            "flowchart TD\n    A -- n>0 --> B\n",
            "flowchart TD\n    A>Order parts] --> B[/Adjust again/]\n",
            "flowchart TD\n    A((x y)) --> B{{x y}} --> C[(x y)] --> D([x y])\n",
            "flowchart TD\n    A[\"x y\"] --> B\n    A@{ shape: rect, label: \"x y\" } --> B\n",
            "flowchart TD\n    A e1@--> B\n    e1@{ animate: true }\n",
            "flowchart TD\n    subgraph Order parts\n    A --> B\n    end\n",
            "flowchart TD\n    subgraph S [Order parts]\n    direction LR\n    end\n",
            "flowchart TD\n    classDef hot fill:#f96,stroke:#333\n    class A,B hot\n",
            "flowchart TD\n    style A fill:#f9f,stroke-width:4px\n    linkStyle 0 stroke:#f00\n",
            "flowchart TD\n    click A \"https://example.org\" \"Order parts\"\n",
            "flowchart TD\n    accTitle: Order parts\n    accDescr: a b c\n",
            "flowchart TD\n    accDescr {\n      Order parts\n      Adjust again\n    }\n    A --> B\n",
            "flowchart TD\n    A[\"`Order\n    parts and more`\"] --> B\n",
            "flowchart TD\n    A@{\n      shape: rect\n      label: \"x y\"\n    }\n",
            "flowchart TD\n    %% Order parts here\n    A --> B %% a b c\n",
            "---\ntitle: Order parts\n---\nflowchart TD\n    A --> B\n",
            "%%{init: {\"flowchart\": {\"curve\": \"basis\"}}}%%\nflowchart TD\n    A --> B\n",
            "%%{init: {\n  \"theme\": \"forest\",\n  \"x\": { \"a b\": 1 }\n}}%%\nflowchart LR\n  A --> B\n",
            "flowchart TD\n    A --o B\n    A --x B\n    A <--> B\n    A -.- B\n    A === B\n",
            "flowchart TD\n    A -- x -- B\n    A --> B --> C\n",
            "flowchart TD\n    title Orders and parts\n    A --> B\n",
            "flowchart TD;\n    A-->B;\n",
            "graph TD\n    A [Order parts] --> B\n",
        ] {
            assert_eq!(wrong(body), None, "{body}");
        }
    }

    #[test]
    fn a_chain_is_cut_at_its_links() {
        let text = "a -->|don't| b --> c";
        let found: Vec<&str> = links(text).iter().map(|l| &text[l.start..l.end]).collect();
        assert_eq!(found, ["-->|don't|", "-->"]);
        let text = "A -- yes --> B -.-> C";
        let found: Vec<&str> = links(text).iter().map(|l| &text[l.start..l.end]).collect();
        assert_eq!(found, ["-- yes -->", "-.->"]);
    }
}
