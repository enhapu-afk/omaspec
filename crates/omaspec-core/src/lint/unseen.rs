//! Characters nobody sees: a zero-width space pasted into a name makes it
//! another name, and every message about it reads as if nothing were
//! wrong. Lint says where one stands and takes it out.
//!
//! A no-break space is not one of them (it is a way to write a text that
//! shows nothing), nor are the joiners and direction marks that scripts
//! and emoji need (U+200C to U+200F), nor a byte order mark at the very
//! start.

use super::{Finding, Fix, Severity};

/// What `c` is called, if it is a character that leaves no mark and has
/// no place in a diagram's text.
fn name(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{ad}' => "soft hyphen",
        '\u{200b}' => "zero-width space",
        '\u{2060}' => "word joiner",
        '\u{feff}' => "zero-width no-break space",
        '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => {
            "a mark that turns the direction of the text"
        }
        '\t' => return None,
        c if c.is_control() => "a control character",
        _ => return None,
    })
}

/// A warning for each line that holds such a character, naming the first
/// and what stands before it.
pub(super) fn findings(lines: &[&str], findings: &mut Vec<Finding>) {
    for (row, line) in lines.iter().enumerate() {
        // The mark a file may begin with is no part of its first line.
        let text = if row == 0 {
            line.trim_start_matches('\u{feff}')
        } else {
            line
        };
        let mut found = text
            .char_indices()
            .filter_map(|(at, c)| Some((at, c, name(c)?)));
        let Some((at, c, called)) = found.next() else {
            continue;
        };
        let more = found.count();
        // The word it ends or stands in, or the one before it.
        let seen: String = text[..at].chars().filter(|c| name(*c).is_none()).collect();
        let place = match seen.split_whitespace().next_back() {
            Some(word) => {
                let from = word.char_indices().rev().nth(19).map_or(0, |(i, _)| i);
                format!("after `{}`", &word[from..])
            }
            None => "at the start of the line".to_owned(),
        };
        let others = match more {
            0 => String::new(),
            1 => " and one more on this line".to_owned(),
            n => format!(" and {n} more on this line"),
        };
        let cleaned: String = text.chars().filter(|c| name(*c).is_none()).collect();
        findings.push(
            Finding::new(
                "unseen-character",
                row,
                Severity::Warning,
                format!(
                    "U+{:04X} ({called}) {place}{others}: nobody sees it, and a name with it is another name.",
                    c as u32
                ),
            )
            .with(Fix::replace("Take out what nobody sees", row, cleaned)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(code: &str) -> Vec<Finding> {
        let lines: Vec<&str> = code.split('\n').collect();
        let mut out = Vec::new();
        findings(&lines, &mut out);
        out
    }

    #[test]
    fn a_zero_width_space_is_named_and_taken_out() {
        let code = "@startuml\nparticipant Al\u{200b}ice\nAlice -> Bob : hi\u{a0}there\n@enduml";
        let all = found(code);
        assert_eq!(all.len(), 1, "{all:?}");
        assert_eq!((all[0].line, all[0].code), (1, "unseen-character"));
        assert!(
            all[0]
                .message
                .starts_with("U+200B (zero-width space) after `Al`:"),
            "{}",
            all[0].message
        );
        let fixed = all[0].fix.as_ref().unwrap().apply(code);
        assert_eq!(fixed.lines().nth(1), Some("participant Alice"));
        assert!(found(&fixed).is_empty());
    }

    #[test]
    fn what_a_text_may_hold_is_left_alone() {
        // A byte order mark at the start, a no-break space, a tab, and the
        // joiners of emoji and scripts.
        assert!(
            found("\u{feff}@startuml\n\tA -> B : a\u{a0}b \u{1f468}\u{200d}\u{1f469}\n@enduml")
                .is_empty()
        );
        let all = found("A -> B\n\u{feff}B -> \u{202e}A\u{2060}");
        assert_eq!(all.len(), 1);
        assert!(
            all[0].message.starts_with(
                "U+FEFF (zero-width no-break space) at the start of the line and 2 more on this line:"
            ),
            "{}",
            all[0].message
        );
    }
}
