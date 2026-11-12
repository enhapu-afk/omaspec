//! Reading EBNF (`@startebnf`): rules `name = expression ;`, the
//! expression of ISO EBNF as PlantUML reads it: `,` between parts (or a
//! space), `|` between ways, `[ … ]` what may be left out, `{ … }` what
//! repeats (`{ … }-` at least once), `( … )` a group, `"…"` and `'…'`
//! words as written, `? … ?` a special sequence, `(* … *)` a comment.

use super::{Diagram, Node};
use crate::Declined;
use crate::common::Frame;
use crate::source::Line;

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Name(String),
    Word(String),
    Special(String),
    Mark(char),
}

pub fn parse(lines: &[Line]) -> Result<Diagram, Declined> {
    let mut frame = Frame::default();
    let mut tokens: Vec<(Token, usize)> = Vec::new();
    let mut open_rule = false;
    let mut at = 0;
    let mut comment = false;
    while at < lines.len() {
        if !open_rule
            && !comment
            && let Some(next) = frame.line(lines, at)?
        {
            at = next;
            continue;
        }
        let Line { no, text, .. } = lines[at];
        comment = scan(text, no, comment, &mut tokens)?;
        open_rule = tokens
            .last()
            .is_some_and(|(t, _)| !matches!(t, Token::Mark(';' | '.')));
        at += 1;
    }
    let mut rules = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let (name, no) = match &tokens[i] {
            (Token::Name(name), no) => (name.clone(), *no),
            (_, no) => return Err(Declined::at(*no, "a rule starts with its name")),
        };
        if !matches!(tokens.get(i + 1), Some((Token::Mark('='), _))) {
            return Err(Declined::at(no, "a rule says `name = …;`"));
        }
        i += 2;
        let node = expression(&tokens, &mut i, no)?;
        match tokens.get(i) {
            Some((Token::Mark(';' | '.'), _)) => i += 1,
            Some((_, at)) => return Err(Declined::at(*at, "a rule ends with `;`")),
            None => return Err(Declined::at(no, "this rule has no `;`")),
        }
        rules.push((Some(name), node));
    }
    if rules.is_empty() {
        return Err(Declined {
            line: None,
            reason: "an EBNF diagram needs a rule: `name = … ;`".into(),
            refused: true,
        });
    }
    Ok(Diagram { rules, frame })
}

/// Cuts `text` into tokens; says whether a comment is still open after it.
fn scan(
    text: &str,
    no: usize,
    mut comment: bool,
    out: &mut Vec<(Token, usize)>,
) -> Result<bool, Declined> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if comment {
            if c == '*' && chars.get(i + 1) == Some(&')') {
                comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if c == '(' && chars.get(i + 1) == Some(&'*') {
            comment = true;
            i += 2;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if matches!(c, '"' | '\'' | '?') {
            let end = chars[i + 1..]
                .iter()
                .position(|&d| d == c)
                .ok_or_else(|| Declined::at(no, format!("`{c}` is not closed on its line")))?;
            let inner: String = chars[i + 1..i + 1 + end].iter().collect();
            out.push((
                if c == '?' {
                    Token::Special(inner.trim().to_owned())
                } else {
                    Token::Word(inner)
                },
                no,
            ));
            i += end + 2;
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            // A name, words apart by single spaces as ISO EBNF allows.
            let mut name = String::new();
            while i < chars.len() {
                let c = chars[i];
                if c.is_alphanumeric() || c == '_' {
                    name.push(c);
                    i += 1;
                } else if c == ' '
                    && chars
                        .get(i + 1)
                        .is_some_and(|d| d.is_alphanumeric() || *d == '_')
                {
                    name.push(' ');
                    i += 1;
                } else {
                    break;
                }
            }
            out.push((Token::Name(name), no));
            continue;
        }
        if "=,|;.[]{}()-*".contains(c) {
            out.push((Token::Mark(c), no));
            i += 1;
            continue;
        }
        return Err(Declined::at(no, format!("`{c}` is not EBNF")));
    }
    Ok(comment)
}

/// `a | b | c`.
fn expression(tokens: &[(Token, usize)], i: &mut usize, no: usize) -> Result<Node, Declined> {
    let mut ways = vec![sequence(tokens, i, no)?];
    while matches!(tokens.get(*i), Some((Token::Mark('|'), _))) {
        *i += 1;
        ways.push(sequence(tokens, i, no)?);
    }
    Ok(Node::choice(ways))
}

/// `a, b, c`, or `a b c`.
fn sequence(tokens: &[(Token, usize)], i: &mut usize, no: usize) -> Result<Node, Declined> {
    let mut parts = Vec::new();
    loop {
        match tokens.get(*i) {
            Some((Token::Mark(','), _)) => *i += 1,
            Some((Token::Mark('|' | ';' | '.' | ')' | ']' | '}'), _)) | None => break,
            _ => parts.push(term(tokens, i, no)?),
        }
    }
    Ok(Node::sequence(parts))
}

fn term(tokens: &[(Token, usize)], i: &mut usize, no: usize) -> Result<Node, Declined> {
    let Some((token, at)) = tokens.get(*i).cloned() else {
        return Err(Declined::at(no, "this rule ends too soon"));
    };
    *i += 1;
    let node = match token {
        Token::Name(name) => Node::NonTerminal(name),
        Token::Word(word) => Node::Terminal(word),
        Token::Special(text) => Node::Special(text),
        Token::Mark(open @ ('[' | '{' | '(')) => {
            let close = match open {
                '[' => ']',
                '{' => '}',
                _ => ')',
            };
            let inner = expression(tokens, i, at)?;
            if !matches!(tokens.get(*i), Some((Token::Mark(c), _)) if *c == close) {
                return Err(Declined::at(at, format!("`{open}` has no `{close}`")));
            }
            *i += 1;
            match open {
                '[' => Node::Optional(Box::new(inner)),
                '{' if matches!(tokens.get(*i), Some((Token::Mark('-'), _))) => {
                    *i += 1;
                    Node::Repeat {
                        node: Box::new(inner),
                        label: None,
                    }
                }
                '{' => Node::any(inner),
                _ => inner,
            }
        }
        Token::Mark(c) => return Err(Declined::at(at, format!("`{c}` cannot stand here"))),
    };
    if matches!(tokens.get(*i), Some((Token::Mark('-'), _))) {
        return Err(Declined::at(at, "an exception (`a - b`) is not drawn yet"));
    }
    Ok(node)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<Line<'_>> {
        text.lines()
            .enumerate()
            .map(|(no, text)| Line {
                no,
                text: text.trim(),
                indent: 0,
                blank_before: 0,
            })
            .collect()
    }

    #[test]
    fn rules_are_read_into_tracks() {
        let d = parse(&lines(
            "title Sums\nsum = term, { (\"+\" | \"-\"), term } ;\nterm = [ \"-\" ] digit\n  (* a comment *) ;\ndigit = ? 0 to 9 ? ;",
        ))
        .unwrap();
        assert_eq!(
            d.frame.title.as_ref().map(|t| t.value.as_str()),
            Some("Sums")
        );
        assert_eq!(d.rules.len(), 3);
        let (name, sum) = &d.rules[0];
        assert_eq!(name.as_deref(), Some("sum"));
        let Node::Sequence(parts) = sum else {
            panic!("{sum:?}")
        };
        assert_eq!(parts[0], Node::NonTerminal("term".into()));
        assert!(
            matches!(&parts[1], Node::Optional(inner) if matches!(**inner, Node::Repeat { .. }))
        );
        assert_eq!(
            d.rules[1].1,
            Node::Sequence(vec![
                Node::Optional(Box::new(Node::Terminal("-".into()))),
                Node::NonTerminal("digit".into())
            ])
        );
        assert_eq!(d.rules[2].1, Node::Special("0 to 9".into()));
    }

    #[test]
    fn what_it_does_not_read_names_its_line() {
        for (text, line, part) in [
            ("a = b", 0, "no `;`"),
            ("a = ( b ;", 0, "no `)`"),
            ("a = b - c ;", 0, "exception"),
            ("= b ;", 0, "starts with its name"),
            ("a = \"b ;", 0, "not closed"),
        ] {
            let err = parse(&lines(text)).unwrap_err();
            assert_eq!(err.line, Some(line), "{text}");
            assert!(err.reason.contains(part), "{text}: {}", err.reason);
        }
    }
}
