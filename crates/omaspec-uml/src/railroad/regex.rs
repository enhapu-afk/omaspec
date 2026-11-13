//! Reading a regular expression (`@startregex`): each line that is no
//! frame line is one, between slashes or without them. Ways apart by `|`,
//! groups (`( … )`, `(?: … )`, `(?<name> … )`), classes (`[a-z_]`,
//! `[^…]`), `.`, the anchors `^` and `$`, the escapes `\d \w \s \b` and
//! escaped characters, and the quantifiers `* + ?` and `{n}`, `{n,}`,
//! `{n,m}`, lazy ones too. Characters in a row are one word.

use super::{Diagram, MAX_DEPTH, Node, too_deep};
use crate::Declined;
use crate::common::Frame;
use crate::source::Line;

pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut frame = Frame::default();
    let mut rules = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        if let Some(next) = frame.line(lines, at)? {
            at = next;
            continue;
        }
        let Line { no, text, .. } = lines[at];
        let body = text
            .strip_prefix('/')
            .and_then(|t| t.rfind('/').map(|end| &t[..end]))
            .unwrap_or(text);
        let chars: Vec<char> = body.chars().collect();
        if depth(&chars) > MAX_DEPTH {
            return Err(too_deep(no));
        }
        let mut i = 0;
        let node = alternation(&chars, &mut i, no)?;
        if i < chars.len() {
            return Err(Declined::at(
                no,
                format!("`{}` cannot stand here", chars[i]),
            ));
        }
        rules.push((None, node));
        at += 1;
    }
    if rules.is_empty() {
        return Err(Declined {
            line: None,
            reason: "a regex diagram needs an expression".into(),
            refused: true,
        });
    }
    Ok(Diagram { rules, frame })
}

/// How deep the groups of `chars` nest, escapes and classes left aside.
fn depth(chars: &[char]) -> usize {
    let (mut open, mut most, mut class) = (0usize, 0, false);
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '[' if !class => class = true,
            ']' if class => class = false,
            '(' if !class => {
                open += 1;
                most = most.max(open);
            }
            ')' if !class => open = open.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    most
}

fn alternation(chars: &[char], i: &mut usize, no: usize) -> Result<Node, Declined> {
    let mut ways = vec![concatenation(chars, i, no)?];
    while chars.get(*i) == Some(&'|') {
        *i += 1;
        ways.push(concatenation(chars, i, no)?);
    }
    Ok(Node::choice(ways))
}

fn concatenation(chars: &[char], i: &mut usize, no: usize) -> Result<Node, Declined> {
    let mut parts: Vec<Node> = Vec::new();
    while let Some(&c) = chars.get(*i) {
        if c == '|' || c == ')' {
            break;
        }
        let atom = atom(chars, i, no)?;
        let node = quantified(atom, chars, i, no)?;
        // Characters in a row read as one word.
        match (parts.last_mut(), &node) {
            (Some(Node::Terminal(before)), Node::Terminal(after)) => before.push_str(after),
            _ => parts.push(node),
        }
    }
    Ok(Node::sequence(parts))
}

fn atom(chars: &[char], i: &mut usize, no: usize) -> Result<Node, Declined> {
    let c = chars[*i];
    *i += 1;
    Ok(match c {
        '(' => {
            if chars.get(*i) == Some(&'?') {
                match chars.get(*i + 1) {
                    Some(':') => *i += 2,
                    Some('<') if !matches!(chars.get(*i + 2), Some('=' | '!')) => {
                        let end = chars[*i..]
                            .iter()
                            .position(|&d| d == '>')
                            .ok_or_else(|| Declined::at(no, "a group's name has no `>`"))?;
                        *i += end + 1;
                    }
                    _ => {
                        return Err(Declined::at(
                            no,
                            "a look-around (`(?=`, `(?!`, `(?<=`) is not drawn yet",
                        ));
                    }
                }
            }
            let inner = alternation(chars, i, no)?;
            if chars.get(*i) != Some(&')') {
                return Err(Declined::at(no, "a group has no `)`"));
            }
            *i += 1;
            inner
        }
        '[' => class(chars, i, no)?,
        '.' => Node::Special("any character".into()),
        '^' => Node::Special("start".into()),
        '$' => Node::Special("end".into()),
        '\\' => escape(chars, i, no)?,
        '*' | '+' | '?' | '{' => {
            return Err(Declined::at(no, format!("`{c}` repeats nothing here")));
        }
        c => Node::Terminal(c.to_string()),
    })
}

/// What follows a backslash.
fn escape(chars: &[char], i: &mut usize, no: usize) -> Result<Node, Declined> {
    let Some(&c) = chars.get(*i) else {
        return Err(Declined::at(no, "a `\\` at the end escapes nothing"));
    };
    *i += 1;
    let class = |items: &[&str], negated: bool| Node::Class {
        items: items.iter().map(|s| (*s).to_owned()).collect(),
        negated,
    };
    Ok(match c {
        'd' => class(&["0-9"], false),
        'D' => class(&["0-9"], true),
        'w' => class(&["a-z", "A-Z", "0-9", "_"], false),
        'W' => class(&["a-z", "A-Z", "0-9", "_"], true),
        's' => class(&["white space"], false),
        'S' => class(&["white space"], true),
        'b' => Node::Special("word boundary".into()),
        'B' => Node::Special("no word boundary".into()),
        't' => Node::Special("tab".into()),
        'n' => Node::Special("new line".into()),
        'r' => Node::Special("return".into()),
        c => Node::Terminal(c.to_string()),
    })
}

/// `[a-z_]`, `[^0-9]`: the items one under another.
fn class(chars: &[char], i: &mut usize, no: usize) -> Result<Node, Declined> {
    let negated = chars.get(*i) == Some(&'^');
    if negated {
        *i += 1;
    }
    let mut items: Vec<String> = Vec::new();
    let mut first = true;
    loop {
        let Some(&c) = chars.get(*i) else {
            return Err(Declined::at(no, "a class has no `]`"));
        };
        *i += 1;
        if c == ']' && !first {
            break;
        }
        first = false;
        let item = if c == '\\' {
            let Some(&e) = chars.get(*i) else {
                return Err(Declined::at(no, "a class has no `]`"));
            };
            *i += 1;
            match e {
                'd' => "0-9".to_owned(),
                'w' => "word character".to_owned(),
                's' => "white space".to_owned(),
                e => e.to_string(),
            }
        } else {
            c.to_string()
        };
        // A range: `a-z`, but not a `-` at the end.
        if chars.get(*i) == Some(&'-') && chars.get(*i + 1).is_some_and(|&d| d != ']') {
            let end = chars[*i + 1];
            *i += 2;
            items.push(format!("{item}-{end}"));
        } else {
            items.push(item);
        }
    }
    Ok(Node::Class { items, negated })
}

/// `atom` with the quantifier after it, if one is.
fn quantified(atom: Node, chars: &[char], i: &mut usize, no: usize) -> Result<Node, Declined> {
    let node = match chars.get(*i) {
        Some('*') => {
            *i += 1;
            Node::any(atom)
        }
        Some('+') => {
            *i += 1;
            Node::Repeat {
                node: Box::new(atom),
                label: None,
            }
        }
        Some('?') => {
            *i += 1;
            Node::Optional(Box::new(atom))
        }
        Some('{') => {
            let end = chars[*i..]
                .iter()
                .position(|&d| d == '}')
                .ok_or_else(|| Declined::at(no, "a `{` has no `}`"))?;
            let inside: String = chars[*i + 1..*i + end].iter().collect();
            let (low, high) = match inside.split_once(',') {
                Some((low, high)) => (low.trim(), Some(high.trim())),
                None => (inside.trim(), None),
            };
            let low: u32 = low
                .parse()
                .map_err(|_| Declined::at(no, format!("`{{{inside}}}` is no count")))?;
            if let Some(high) = high.filter(|h| !h.is_empty())
                && high.parse::<u32>().is_err()
            {
                return Err(Declined::at(no, format!("`{{{inside}}}` is no count")));
            }
            *i += end + 1;
            let repeat = Node::Repeat {
                node: Box::new(atom),
                label: Some(format!("{{{inside}}}")),
            };
            if low == 0 {
                Node::Optional(Box::new(repeat))
            } else {
                repeat
            }
        }
        _ => return Ok(atom),
    };
    // A lazy quantifier is drawn as the greedy one.
    if chars.get(*i) == Some(&'?') {
        *i += 1;
    }
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> Result<Node, Declined> {
        let lines = [Line {
            no: 0,
            text,
            indent: 0,
            blank_before: 0,
        }];
        parse(&lines).map(|mut d| d.rules.remove(0).1)
    }

    #[test]
    fn an_expression_is_read_into_a_track() {
        let node = one("/[a-z0-9._-]+@[a-z]+\\.(com|org){1,2}/").unwrap();
        let Node::Sequence(parts) = node else {
            panic!()
        };
        assert!(
            matches!(&parts[0], Node::Repeat { node, .. } if matches!(**node, Node::Class { ref items, .. } if items == &["a-z", "0-9", ".", "_", "-"]))
        );
        assert_eq!(parts[1], Node::Terminal("@".into()));
        assert_eq!(parts[3], Node::Terminal(".".into()));
        assert!(matches!(&parts[4], Node::Repeat { label: Some(l), .. } if l == "{1,2}"));
        assert_eq!(one("abc").unwrap(), Node::Terminal("abc".into()), "a word");
        assert!(matches!(one("^\\d*$").unwrap(), Node::Sequence(p) if p.len() == 3));
        assert!(matches!(
            one("[^0-9]").unwrap(),
            Node::Class { negated: true, .. }
        ));
    }

    #[test]
    fn what_it_does_not_read_says_so() {
        for (text, part) in [
            ("(ab", "no `)`"),
            ("[ab", "no `]`"),
            ("a(?=b)", "look-around"),
            ("*a", "repeats nothing"),
            ("a{x}", "no count"),
        ] {
            let err = one(text).unwrap_err();
            assert!(err.reason.contains(part), "{text}: {}", err.reason);
        }
    }
}
