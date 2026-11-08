//! From a label of the drawing back to the source line that made it, for
//! Ctrl+click in the preview. The caret goes to the first character of that
//! label on the line, [`column_of`], so an earlier word on the same line is
//! not what lights up. The other way, [`aimed`] is the drawn label the caret
//! stands in, so the sheet can light that shape. [`word_at`] is the single
//! word at a column.
//!
//! The drawing only has the label's text. The line is the one holding that
//! text, the n-th such line for the n-th such label; whole words count
//! before parts of words (`CurrentDemand` is not `CurrentDemandReq`), the
//! exact case before any case. Comment lines draw nothing and never match.
//! Brackets an engine draws around a label (a loop's `[three times]`) are
//! dropped when the label is not found with them. A line whose PlantUML
//! markup (`**bold**`, `<i>`, `<color:red>`) splits the label finds it
//! once the markup is gone. A label wrapped over several lines, such as an
//! activity written `:…` and ended with `;`, is that label from any of its lines.
//! A `note` is that note from any column of it, including the participants
//! it is placed over.

mod aim;
mod note;
mod text;
mod wrap;

pub use aim::aimed;

/// The zero-based line that draws `label`, the `nth` (from 0) label with
/// that text in the drawing.
pub fn line_of(code: &str, label: &str, nth: usize) -> Option<usize> {
    column_of(code, label, nth).map(|(line, _)| line)
}

/// The line [`line_of`] returns, and the byte on that line where the matched
/// text starts. A caret there stands on the label. `On --> Updating` clicked
/// as Updating lands on `Updating`, not on `On`.
pub fn column_of(code: &str, label: &str, nth: usize) -> Option<(usize, usize)> {
    let shown: Vec<Option<text::Norm>> = code
        .split('\n')
        .map(|line| text::read(line, false))
        .collect();
    let plain: Vec<Option<text::Norm>> = code
        .split('\n')
        .map(|line| text::read(line, true))
        .collect();
    let bare = label
        .strip_prefix(['[', '(', '"', '«', '<'])
        .and_then(|l| l.strip_suffix([']', ')', '"', '»', '>']))
        .filter(|l| !l.trim().is_empty());
    std::iter::once(label)
        .chain(bare)
        .find_map(|label| by_words(&shown, &plain, label, nth))
}

fn by_words(
    lines: &[Option<text::Norm>],
    plain: &[Option<text::Norm>],
    label: &str,
    nth: usize,
) -> Option<(usize, usize)> {
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

/// How a line is allowed to hold the label. A whole word outranks a part of
/// one, and the written case outranks any case. The first tier that hits a
/// line wins, for every line, so a later exact word beats an earlier guess.
#[derive(Clone, Copy)]
enum Tier {
    Word,
    Part,
    WordAnyCase,
    PartAnyCase,
}

const TIERS: [Tier; 4] = [Tier::Word, Tier::Part, Tier::WordAnyCase, Tier::PartAnyCase];

fn find(lines: &[Option<text::Norm>], wanted: &str, nth: usize) -> Option<(usize, usize)> {
    let lower = wanted.to_lowercase();
    TIERS.iter().find_map(|tier| {
        let hits: Vec<(usize, usize, usize)> = lines
            .iter()
            .enumerate()
            .filter_map(|(index, norm)| {
                // A comment is `None` and draws nothing, so it never matches.
                let (from, to) = span_in(&norm.as_ref()?.text, wanted, &lower, *tier)?;
                Some((index, from, to))
            })
            .collect();
        let (index, from, to) = hits.get(nth).copied().or_else(|| hits.last().copied())?;
        let (start, _) = lines.get(index)?.as_ref()?.map(from, to)?;
        Some((index, start))
    })
}

/// Where `wanted` sits in the normalized `line`, in that line's own bytes.
/// A case-folded hit is carried back onto `line`, since lowercasing can
/// change how many bytes a character takes.
fn span_in(line: &str, wanted: &str, lower: &str, tier: Tier) -> Option<(usize, usize)> {
    if wanted.is_empty() {
        return None;
    }
    match tier {
        Tier::Word => text::locate_in(line, wanted, false).into_iter().next(),
        Tier::Part => line.find(wanted).map(|at| (at, at + wanted.len())),
        Tier::WordAnyCase => {
            let folded = line.to_lowercase();
            let (from, to) = text::locate_in(&folded, lower, false).into_iter().next()?;
            unfold(line, from, to)
        }
        Tier::PartAnyCase => {
            let folded = line.to_lowercase();
            let at = folded.find(lower)?;
            unfold(line, at, at + lower.len())
        }
    }
}

/// `folded[from..to]` of `text.to_lowercase()`, as bytes of `text`.
fn unfold(text: &str, lower_from: usize, lower_to: usize) -> Option<(usize, usize)> {
    if lower_from >= lower_to {
        return None;
    }
    let mut at = 0;
    let mut start = None;
    let mut end = None;
    for (index, ch) in text.char_indices() {
        let piece: String = ch.to_lowercase().collect();
        let next = at + piece.len();
        if start.is_none() && lower_from < next && lower_to > at {
            start = Some(index);
        }
        if start.is_some() && lower_to <= next {
            end = Some(index + ch.len_utf8());
            break;
        }
        at = next;
    }
    Some((start?, end.unwrap_or(text.len())))
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

    #[test]
    fn the_caret_lands_on_the_clicked_word() {
        let code = "\
stateDiagram-v2
  [*] --> Off
  Off --> On : power
  state On {
    [*] --> Stopped
    Stopped --> Playing : play
    state Playing {
      [*] --> Buffering
      Buffering --> Streaming : buffer full
      Streaming --> Buffering : network slow
    }
  }
  On --> Off : power
  On --> Updating : update ready
  Updating --> On : done
  Updating --> Off : failed
";
        let (line, col) = column_of(code, "Updating", 0).unwrap();
        let row = code.split('\n').nth(line).unwrap();
        assert!(row[col..].starts_with("Updating"), "{row}");
        assert_eq!(word_at(row, col), Some("Updating"));
        // The first word of that line is a different state.
        let indent = row.len() - row.trim_start().len();
        assert_eq!(word_at(row, indent), Some("On"));
        let labels = [
            "Off",
            "On",
            "power",
            "Stopped",
            "Playing",
            "play",
            "Buffering",
            "Streaming",
            "buffer full",
            "network slow",
            "Updating",
            "update ready",
            "done",
            "failed",
        ];
        assert_eq!(
            aimed(code, line, col, labels),
            Some(("Updating".to_owned(), 0))
        );
        assert_eq!(
            aimed(code, line, indent, labels).map(|(label, _)| label),
            Some("On".to_owned())
        );

        let (line, col) = column_of(code, "buffer full", 0).unwrap();
        let row = code.split('\n').nth(line).unwrap();
        assert!(row[col..].starts_with("buffer full"), "{row}");

        let code = "sequenceDiagram\n  Alice->>Bob: hi\n  Bob->>Alice: hi\n";
        let (line, col) = column_of(code, "hi", 1).unwrap();
        assert_eq!(line, 2);
        assert!(code.split('\n').nth(line).unwrap()[col..].starts_with("hi"));

        let code = "flowchart LR\n  A[First<br>Second] --> B[\"a &lt; b\"]\n";
        let (line, col) = column_of(code, "First Second", 0).unwrap();
        assert!(code.split('\n').nth(line).unwrap()[col..].starts_with("First"));
        let (line, col) = column_of(code, "a < b", 0).unwrap();
        assert!(code.split('\n').nth(line).unwrap()[col..].starts_with("a "));

        let code = "*** in **bold** and //italic//";
        let (line, col) = column_of(code, "in bold and italic", 0).unwrap();
        assert_eq!(line, 0);
        assert!(code[col..].starts_with("in "));

        let code = "state Ähre";
        let (line, col) = column_of(code, "ÄHRE", 0).unwrap();
        assert_eq!(line, 0);
        assert!(code[col..].starts_with("Ähre"));

        let code = "sequenceDiagram\n  loop three times\n  end";
        let (_, col) = column_of(code, "[three times]", 0).unwrap();
        assert!(code.split('\n').nth(1).unwrap()[col..].starts_with("three"));
    }
}
