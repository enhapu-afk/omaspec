//! A flowchart as mmdr is given it to read.
//!
//! mmdr skips a line that starts with `title `, a title in other kinds,
//! though in a flowchart `title --> B` links a node named `title`: the
//! line was lost without a word. And it takes a `-`, `=` or `&` in the
//! text of an asymmetric box, `A>svc-30]`, for a link, and draws two
//! boxes named `A>svc` and `30]`. Both are written here so mmdr reads them
//! as Mermaid does: the space after `title` as a tab, the box's text in
//! quotes. Every line keeps its number; only mmdr reads the copy.

use std::borrow::Cow;

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
    quoted.or(titled)
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
    fn the_lines_keep_their_numbers() {
        let body = "flowchart LR\n  title --> A>a-b]\n\n  A --> C\n";
        let read = readable(body);
        assert_eq!(read.lines().count(), body.lines().count());
        assert_eq!(read.lines().nth(1), Some("  title\t--> A>\"a-b\"]"));
    }
}
