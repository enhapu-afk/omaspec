//! A label the drawing wraps across several lines of the source.
//!
//! An activity's text is one label. The source splits it after `:`, and the
//! last line ends with `;`. Each of those lines is that label.

use super::text;

/// One wrapped occurrence: the drawn label, and the source lines that write it.
pub(super) struct Run<'a> {
    pub label: &'a str,
    pub rows: Vec<usize>,
}

/// Every label `labels` draws by wrapping it over two or more lines.
pub(super) fn runs<'a>(source: &str, labels: &[&'a str]) -> Vec<Run<'a>> {
    let lines: Vec<Words> = source.split('\n').map(words_of).collect();
    let mut order: Vec<&str> = labels
        .iter()
        .copied()
        .filter(|label| label.split_whitespace().nth(1).is_some())
        .collect();
    order.sort_by(|a, b| {
        b.split_whitespace()
            .count()
            .cmp(&a.split_whitespace().count())
            .then(b.len().cmp(&a.len()))
    });
    order.dedup();
    let mut used = vec![false; lines.len()];
    let mut found = Vec::new();
    for label in order {
        let words: Vec<&str> = label.split_whitespace().collect();
        let mut row = 0;
        while row < lines.len() {
            if used[row] {
                row += 1;
                continue;
            }
            let Some(taken) = consume(&lines, row, &words) else {
                row += 1;
                continue;
            };
            let last = taken[taken.len() - 1];
            if taken.len() >= 2 {
                for index in &taken {
                    used[*index] = true;
                }
                found.push(Run { label, rows: taken });
            }
            row = last + 1;
        }
    }
    found
}

enum Words {
    /// A blank line may sit inside a wrapped action.
    Blank,
    /// A comment draws nothing and ends a wrap.
    Stop,
    Text(Vec<String>),
}

fn words_of(line: &str) -> Words {
    if line.trim().is_empty() {
        return Words::Blank;
    }
    if text::comment(line) {
        return Words::Stop;
    }
    let Some(norm) = text::read(line, true) else {
        return Words::Stop;
    };
    let bare = bare(&norm.text);
    if bare.is_empty() {
        Words::Blank
    } else {
        Words::Text(bare.split_whitespace().map(str::to_owned).collect())
    }
}

/// The line without the `:` or `#color:` an action starts with, and without
/// the `;` or `; <<shape>>` it ends with. The drawing leaves those out.
fn bare(text: &str) -> &str {
    let mut text = text.trim();
    if let Some(rest) = text.strip_prefix(':') {
        text = rest.trim_start();
    } else if let Some(rest) = text.strip_prefix('#')
        && let Some(end) = rest.find(':')
    {
        let name = &rest[..end];
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/' | '\\' | '|'))
        {
            text = rest[end + 1..].trim_start();
        }
    }
    let text = text.trim_end();
    if let Some(inner) = text.strip_suffix(">>")
        && let Some(open) = inner.rfind("<<")
        && let Some(body) = inner[..open].trim_end().strip_suffix(';')
    {
        return body.trim_end();
    }
    text.strip_suffix(';').unwrap_or(text).trim_end()
}

/// Source rows, from `from`, whose words are exactly `words`. A blank line
/// in between stays inside the wrap.
fn consume(lines: &[Words], from: usize, words: &[&str]) -> Option<Vec<usize>> {
    let mut at = 0;
    let mut taken = Vec::new();
    let mut row = from;
    while at < words.len() && row < lines.len() {
        match &lines[row] {
            Words::Blank if !taken.is_empty() => row += 1,
            Words::Text(have) => {
                let n = have.len();
                if at + n > words.len()
                    || have
                        .iter()
                        .zip(&words[at..at + n])
                        .any(|(have, want)| have != want)
                {
                    return None;
                }
                taken.push(row);
                at += n;
                row += 1;
            }
            Words::Blank | Words::Stop => return None,
        }
    }
    (at == words.len()).then_some(taken)
}
