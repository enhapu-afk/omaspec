//! From a label of the drawing back to the source line that made it, for
//! Ctrl+click in the preview. The other way, [`aimed`] is the drawn label
//! the caret stands in, so the sheet can light that shape. [`word_at`] is
//! the single word at a column.
//!
//! The drawing only has the label's text. The line is the one holding that
//! text, the n-th such line for the n-th such label; whole words count
//! before parts of words (`CurrentDemand` is not `CurrentDemandReq`), the
//! exact case before any case. Comment lines draw nothing and never match.
//! Brackets an engine draws around a label (a loop's `[three times]`) are
//! dropped when the label is not found with them. A line whose PlantUML
//! markup (`**bold**`, `<i>`, `<color:red>`) splits the label finds it
//! once the markup is gone.

mod aim;
mod text;

pub use aim::aimed;

/// The zero-based line that draws `label`, the `nth` (from 0) label with
/// that text in the drawing.
pub fn line_of(code: &str, label: &str, nth: usize) -> Option<usize> {
    let lines: Vec<String> = code
        .split('\n')
        .map(|line| text::visible(line, false))
        .collect();
    let plain: Vec<String> = code
        .split('\n')
        .map(|line| text::visible(line, true))
        .collect();
    let bare = label
        .strip_prefix(['[', '(', '"', '«', '<'])
        .and_then(|l| l.strip_suffix([']', ')', '"', '»', '>']))
        .filter(|l| !l.trim().is_empty());
    std::iter::once(label)
        .chain(bare)
        .find_map(|label| by_words(&lines, &plain, label, nth))
}

fn by_words(lines: &[String], plain: &[String], label: &str, nth: usize) -> Option<usize> {
    let words: Vec<&str> = label.split_whitespace().collect();
    // A label too long for its box may be cut or wrapped differently than
    // written; fewer words still find the line.
    (1..=words.len()).rev().find_map(|n| {
        let wanted = words[..n].join(" ");
        if n < words.len() && wanted.chars().count() < 3 {
            return None;
        }
        find(lines, &wanted, nth).or_else(|| find(plain, &wanted, nth))
    })
}

fn find(lines: &[String], wanted: &str, nth: usize) -> Option<usize> {
    let lower = wanted.to_lowercase();
    let tiers: [&dyn Fn(&str) -> bool; 4] = [
        &|line| has_word(line, wanted),
        &|line| line.contains(wanted),
        &|line| has_word(&line.to_lowercase(), &lower),
        &|line| line.to_lowercase().contains(&lower),
    ];
    tiers.iter().find_map(|matches| {
        let hits: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| matches(line))
            .map(|(i, _)| i)
            .collect();
        hits.get(nth).or(hits.last()).copied()
    })
}

/// Whether `wanted` stands in `line` on its own: no letter or digit glued
/// to its ends (where its ends are letters or digits).
fn has_word(line: &str, wanted: &str) -> bool {
    !text::locate_in(line, wanted, false).is_empty()
}

/// The word the caret is on in `line`. `col` is a byte offset: the caret
/// counts in the middle of a word and just after its last letter. A word
/// is letters, digits and `_`. A column past the line is its end; one
/// inside a character moves back to the character's start.
pub fn word_at(line: &str, col: usize) -> Option<&str> {
    let (start, end) = word_range(line, col)?;
    Some(&line[start..end])
}

/// The word around `col`, as source bytes. The same rules as [`word_at`].
pub(super) fn word_range(line: &str, col: usize) -> Option<(usize, usize)> {
    let col = floor_char(line, col.min(line.len()));
    let on_word = line[col..].chars().next().is_some_and(is_name);
    let (start, end) = if on_word {
        (name_start(line, col), name_end(line, col))
    } else if line[..col].chars().next_back().is_some_and(is_name) {
        (name_start(line, col), col)
    } else {
        return None;
    };
    (start < end).then_some((start, end))
}

pub(super) fn is_name(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The first byte of the word that reaches `col`.
pub(super) fn name_start(line: &str, col: usize) -> usize {
    let mut start = col;
    for (index, c) in line[..col].char_indices().rev() {
        if !is_name(c) {
            break;
        }
        start = index;
    }
    start
}

/// The byte after the word that starts at `col`.
pub(super) fn name_end(line: &str, col: usize) -> usize {
    let mut end = col;
    for (index, c) in line[col..].char_indices() {
        if !is_name(c) {
            break;
        }
        end = col + index + c.len_utf8();
    }
    end
}

/// `col` moved back onto a character boundary.
pub(super) fn floor_char(line: &str, mut col: usize) -> usize {
    while col > 0 && !line.is_char_boundary(col) {
        col -= 1;
    }
    col
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nth_label_is_the_nth_line() {
        let code = "sequenceDiagram\n  Alice->>Bob: hi\n  Bob->>Alice: hi\n";
        assert_eq!(line_of(code, "hi", 0), Some(1));
        assert_eq!(line_of(code, "hi", 1), Some(2));
        // More labels than lines: the last line.
        assert_eq!(line_of(code, "hi", 5), Some(2));
        assert_eq!(line_of(code, "nowhere", 0), None);
    }

    #[test]
    fn whole_words_before_parts_and_case_before_any_case() {
        let code = "@startuml\nA -> B : CurrentDemandReq\nloop [CurrentDemand]\nend\n@enduml";
        assert_eq!(line_of(code, "[CurrentDemand]", 0), Some(2));
        assert_eq!(line_of(code, "CurrentDemand", 0), Some(2));
        // Brackets drawn around a title that has none in the source.
        let code = "sequenceDiagram\n  loop three times\n  end";
        assert_eq!(line_of(code, "[three times]", 0), Some(1));
        let code = "state idle\nstate Idle";
        assert_eq!(line_of(code, "Idle", 0), Some(1));
        assert_eq!(line_of(code, "IDLE", 0), Some(0));
    }

    #[test]
    fn line_breaks_entities_and_comments() {
        let code = "flowchart LR\n  %% First Second\n  A[First<br>Second] --> B[\"a &lt; b\"]\n";
        assert_eq!(line_of(code, "First Second", 0), Some(2));
        assert_eq!(line_of(code, "a < b", 0), Some(2));
        let code = "@startuml\n' Login\n:Log in\\nwith a password;\n@enduml";
        assert_eq!(line_of(code, "Log in with a password", 0), Some(2));
    }

    #[test]
    fn plantuml_markup_does_not_hide_a_label() {
        let code = "@startmindmap\n* root\n*** in **bold** and //italic//\n** <color:red>red</color> ~**star\n@endmindmap";
        assert_eq!(line_of(code, "in bold and italic", 0), Some(2));
        assert_eq!(line_of(code, "red **star", 0), Some(3));
        // Written as is, it still matches first.
        assert_eq!(line_of("a //b//\nhttp://x", "http://x", 0), Some(1));
    }

    #[test]
    fn the_word_at_the_caret() {
        let line = "state Player {";
        let at = line.find("Player").unwrap();
        assert_eq!(word_at(line, at), Some("Player"));
        assert_eq!(word_at(line, at + 3), Some("Player"));
        // Just after the last letter: the caret sits on the space.
        assert_eq!(word_at(line, at + "Player".len()), Some("Player"));
        assert_eq!(word_at(line, line.find('{').unwrap()), None);
        assert_eq!(word_at(line, 0), Some("state"));
        assert_eq!(word_at("  ", 1), None);
        assert_eq!(word_at("->", 1), None);
        assert_eq!(word_at("a_b", 2), Some("a_b"));
        assert_eq!(word_at("A", 0), Some("A"));
        assert_eq!(word_at("A", 1), Some("A"));
        assert_eq!(word_at("Go", 100), Some("Go"));
        // Ä is two bytes; a column inside it still belongs to the word.
        let line = "Ähre";
        assert_eq!(word_at(line, 0), Some("Ähre"));
        assert_eq!(word_at(line, 1), Some("Ähre"));
        assert_eq!(word_at(line, line.len()), Some("Ähre"));
    }

    #[test]
    fn a_label_drawn_differently_is_found_by_its_first_words() {
        let code = "@startuml\nA -> B : a rather long message text\n@enduml";
        assert_eq!(line_of(code, "a rather long message te…", 0), Some(1));
        assert_eq!(line_of("abc def\nxyz", "abc zzz", 0), Some(0));
        // A cut down to one or two letters is no evidence.
        assert_eq!(line_of("x\ny", "y z", 0), None);
    }
}
