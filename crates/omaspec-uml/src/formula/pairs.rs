//! Which brackets close which: TeX pairs brackets of either kind on one
//! level, AsciiMath a `|` with the next on its level.

use super::symbols::ascii_name;

/// Where the TeX `(` and `[` start that a `)` or `]` closes on their level.
/// TeX pairs no brackets, so `[0, 1)`, `]0, 1[` and `(a` are all
/// formulas; a pair is one fence, which grows around a tall subformula.
pub(super) fn tex_pairs(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    // `Some` is a bracket that may pair, `None` a group it cannot leave.
    let mut open: Vec<Option<usize>> = Vec::new();
    let mut pairs = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        i += 1;
        match c {
            b'\\' => {
                let word = bytes[i..]
                    .iter()
                    .take_while(|b| b.is_ascii_alphabetic())
                    .count();
                let name = &text[i..i + word];
                i += word.max(1);
                match name {
                    "" if bytes.get(i - 1) == Some(&b'{') => open.push(None),
                    "" if bytes.get(i - 1) == Some(&b'}') => close_group(&mut open),
                    "left" | "right" => {
                        // The delimiter belongs to the command.
                        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
                            i += 1;
                        }
                        i += if bytes.get(i) == Some(&b'\\') { 2 } else { 1 };
                        if name == "left" {
                            open.push(None);
                        } else {
                            close_group(&mut open);
                        }
                    }
                    _ => {}
                }
            }
            b'(' | b'[' => open.push(Some(i - 1)),
            b')' | b']' => {
                if let Some(Some(at)) = open.last() {
                    pairs.push(*at);
                    open.pop();
                }
            }
            b'{' => open.push(None),
            b'}' => close_group(&mut open),
            _ => {}
        }
    }
    pairs.sort_unstable();
    pairs
}

/// Where the AsciiMath `|` start that a second `|` closes on their level;
/// any other bar is drawn as it is, as in `P(A|B)` and `{x | x > 0}`.
pub(super) fn bar_pairs(text: &str) -> Vec<usize> {
    // `Some` is a bar that may pair, `None` a bracket it cannot leave.
    let mut open: Vec<Option<usize>> = Vec::new();
    let mut pairs = Vec::new();
    let mut at = 0;
    while let Some(c) = text[at..].chars().next() {
        // A name such as `|->` is not a bar; a name of letters holds none.
        if !c.is_alphanumeric()
            && !c.is_whitespace()
            && let Some((name, _)) = ascii_name(&text[at..])
        {
            at += name.len();
            continue;
        }
        match c {
            '(' | '[' | '{' => open.push(None),
            ')' | ']' | '}' => close_group(&mut open),
            '|' => {
                if let Some(Some(start)) = open.last() {
                    pairs.push(*start);
                    open.pop();
                } else {
                    open.push(Some(at));
                }
            }
            '"' => {
                if let Some(end) = text[at + 1..].find('"') {
                    at += end + 1;
                }
            }
            _ => {}
        }
        at += c.len_utf8();
    }
    pairs.sort_unstable();
    pairs
}

/// Ends the innermost group, and with it the openers inside that found no
/// partner.
pub(super) fn close_group(open: &mut Vec<Option<usize>>) {
    while let Some(Some(_)) = open.pop() {}
}
