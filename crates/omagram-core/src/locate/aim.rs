//! The drawn label the caret stands in.
//!
//! Needles are built once. Only the caret's line is searched for every label;
//! earlier copies of the chosen label are counted on their own. An alias is
//! only the ident `as` names, and the first declaration in the file wins. A
//! `{...}`, `(...)` or `<<...>>` counts only when nothing but space stands
//! between it and the label.

use std::collections::HashMap;

use super::text::{self, Norm};

/// An alphanumeric run this long is enough to skip a line that lacks it.
const ANCHOR: usize = 4;

/// How a drawn string was found in a source line. A higher kind wins when
/// two finds cover the same columns.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    /// A wrapped label's first or last line, and that line holds nothing else.
    Fragment,
    /// The drawing cut the label and left an ellipsis.
    Ellipsis,
    /// The drawing's brackets or quotes were set aside.
    Unwrapped,
    /// The drawn characters themselves.
    Exact,
}

#[derive(Clone)]
struct Occ<'a> {
    text: &'a str,
    start: usize,
    end: usize,
    /// The ident `as` gives this label, when it sits outside the label's own text.
    also: Vec<(usize, usize)>,
    /// That same ident, for lines that name the label without drawing it.
    alias: Option<(usize, usize)>,
    kind: Kind,
}

impl Occ<'_> {
    fn covers(&self, col: usize) -> bool {
        self.parts().any(|(start, end)| col >= start && col <= end)
    }

    fn strict(&self, col: usize) -> bool {
        self.parts().any(|(start, end)| col >= start && col < end)
    }

    fn parts(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        std::iter::once((self.start, self.end)).chain(self.also.iter().copied())
    }
}

struct Chosen<'a> {
    text: &'a str,
    start: usize,
}

/// One search string of one label.
struct Probe {
    label: usize,
    kind: Kind,
    needle: String,
}

/// A find on a line, before shorter labels inside longer ones are dropped.
struct Hit<'a> {
    label: usize,
    occ: Occ<'a>,
}

/// Labels prepared once per caret lookup.
struct Prep<'a> {
    labels: Vec<&'a str>,
    probes: Vec<Probe>,
    /// Alphanumeric run -> probes that contain it.
    anchored: HashMap<String, Vec<usize>>,
    /// Probes with no run long enough to reject a line.
    shorts: Vec<usize>,
    /// Whole normalized line -> labels for which that line is a wrapped piece.
    fragments: HashMap<String, Vec<usize>>,
    /// Long runs of each label, longest first.
    anchors_of: Vec<Vec<String>>,
    /// Wrapped pieces of each label.
    fragments_of: Vec<Vec<String>>,
    /// The label has a needle too short to reject by a run.
    has_short: Vec<bool>,
    /// Probe indexes of each label.
    probes_of: Vec<Vec<usize>>,
}

impl<'a> Prep<'a> {
    fn build(texts: impl IntoIterator<Item = &'a str>) -> Self {
        let mut labels: Vec<&str> = texts.into_iter().filter(|text| !text.is_empty()).collect();
        labels.sort_unstable();
        labels.dedup();
        let mut probes = Vec::new();
        let mut anchored: HashMap<String, Vec<usize>> = HashMap::new();
        let mut shorts = Vec::new();
        let mut by_line: HashMap<String, Vec<usize>> = HashMap::new();
        let mut anchors_of = Vec::new();
        let mut fragments_of = Vec::new();
        let mut has_short = Vec::new();
        let mut probes_of = Vec::new();
        for (label_index, label) in labels.iter().copied().enumerate() {
            let mut runs = Vec::new();
            let mut short = false;
            let mut mine = Vec::new();
            for (kind, needle) in candidates(label) {
                let found = anchor_runs(&needle);
                if found.is_empty() {
                    short = true;
                    shorts.push(probes.len());
                } else if let Some(longest) = found.into_iter().max_by_key(String::len) {
                    // One run is enough: a hit contains every run of its needle,
                    // so the longest one rejects lines that cannot match.
                    anchored
                        .entry(longest.clone())
                        .or_default()
                        .push(probes.len());
                    runs.push(longest);
                }
                mine.push(probes.len());
                probes.push(Probe {
                    label: label_index,
                    kind,
                    needle,
                });
            }
            let mut pieces = Vec::new();
            for piece in fragments(label) {
                by_line.entry(piece.clone()).or_default().push(label_index);
                pieces.push(piece);
            }
            runs.sort_by_key(|run| std::cmp::Reverse(run.len()));
            runs.dedup();
            anchors_of.push(runs);
            fragments_of.push(pieces);
            has_short.push(short);
            probes_of.push(mine);
        }
        Self {
            labels,
            probes,
            anchored,
            shorts,
            fragments: by_line,
            anchors_of,
            fragments_of,
            has_short,
            probes_of,
        }
    }
}

/// The drawn label the caret is in, and which copy of it (from 0) this
/// mention is. `row` is a zero-based line, `col` a byte on that line.
/// `texts` are the labels the sheet actually drew.
///
/// The caret counts in the middle of a label and just after its last
/// character. A name with spaces, a state in parentheses, a guard the
/// drawing wraps in brackets, and a `<<stereotype>>` the drawing writes
/// with guillemets all count. So does the ident of `as`, and a `{modifier}`
/// or `<<mark>>` with only space between it and a name on its line. A
/// comment line matches nothing.
pub fn aimed<'a>(
    source: &str,
    row: usize,
    col: usize,
    texts: impl IntoIterator<Item = &'a str>,
) -> Option<(String, usize)> {
    let line = source.split('\n').nth(row)?;
    if text::comment(line) {
        return None;
    }
    let col = super::floor_char(line, col.min(line.len()));
    let prep = Prep::build(texts);
    let occs = line_occs(line, &prep);
    let aliases = alias_map(source, &prep);
    let chosen = best_covering(&occs, col)
        .or_else(|| alias_use(line, col, &aliases))
        .or_else(|| nearest_bracket(line, col, &occs))?;
    let nth = count_before(source, row, chosen.text, chosen.start, &aliases, &prep);
    Some((chosen.text.to_owned(), nth))
}

/// `(alias, label)` in file order. The first declaration of a name wins.
fn alias_map<'a>(source: &str, prep: &Prep<'a>) -> Vec<(String, &'a str)> {
    let mut aliases = Vec::new();
    for line in source.split('\n') {
        if text::comment(line) || !has_as_word(line) {
            continue;
        }
        for occ in line_occs(line, prep) {
            let Some((from, to)) = occ.alias else {
                continue;
            };
            let name = &line[from..to];
            if aliases.iter().any(|(have, _)| have == name) {
                continue;
            }
            aliases.push((name.to_owned(), occ.text));
        }
    }
    aliases
}

/// Copies of `label` before `start` on `row`, alias uses included.
fn count_before(
    source: &str,
    row: usize,
    label: &str,
    start: usize,
    aliases: &[(String, &str)],
    prep: &Prep<'_>,
) -> usize {
    let Some(index) = prep.labels.iter().position(|text| *text == label) else {
        return 0;
    };
    let alias = aliases
        .iter()
        .find(|(_, text)| *text == label)
        .map(|(name, _)| name.as_str());
    let mut seen = 0;
    for (at_row, line) in source.split('\n').enumerate() {
        if at_row > row {
            break;
        }
        let holds = worth(line, index, prep);
        let alias_here = alias.is_some_and(|name| line.contains(name));
        if !alias_here && !holds {
            continue;
        }
        // The whole sheet is read only where this label itself occurs, so a
        // shared word does not pull every other label onto the line.
        let occs = if holds && label_present(line, index, prep) {
            line_occs(line, prep)
        } else {
            Vec::new()
        };
        let mut at = Vec::new();
        for occ in &occs {
            if occ.text == label {
                at.push(occ.start);
            }
        }
        if let Some(alias) = alias {
            for (from, _) in text::locate_in(line, alias, false) {
                let covered = occs.iter().any(|occ| occ.text == label && occ.covers(from));
                if !covered {
                    at.push(from);
                }
            }
        }
        at.sort_unstable();
        for pos in at {
            if at_row == row && pos >= start {
                return seen;
            }
            seen += 1;
        }
    }
    seen
}

/// Whether `line` can hold label `index` once markup is read.
fn worth(line: &str, index: usize, prep: &Prep<'_>) -> bool {
    if prep.has_short[index] || marked(line) {
        return true;
    }
    prep.anchors_of[index].iter().any(|run| line.contains(run))
        || prep.fragments_of[index]
            .iter()
            .any(|piece| line.contains(piece))
}

/// Bytes that [`text::read`] can turn into different letters.
fn marked(line: &str) -> bool {
    line.contains('&')
        || line.contains('<')
        || line.contains('\\')
        || line.contains('~')
        || line.contains("**")
        || line.contains("//")
        || line.contains("__")
        || line.contains("\"\"")
}

/// Whether label `index` occurs on `line`, ignoring labels drawn around it.
fn label_present(line: &str, index: usize, prep: &Prep<'_>) -> bool {
    let Some(raw) = text::read(line, false) else {
        return false;
    };
    if norm_has_label(&raw, index, prep) {
        return true;
    }
    text::read(line, true)
        .is_some_and(|plain| plain.text != raw.text && norm_has_label(&plain, index, prep))
}

fn norm_has_label(norm: &Norm, index: usize, prep: &Prep<'_>) -> bool {
    if prep
        .fragments
        .get(&norm.text)
        .is_some_and(|labels| labels.contains(&index))
    {
        return true;
    }
    prep.probes_of[index].iter().any(|&probe_at| {
        let probe = &prep.probes[probe_at];
        !text::locate_in(&norm.text, &probe.needle, probe.kind == Kind::Ellipsis).is_empty()
    })
}

fn has_as_word(line: &str) -> bool {
    let chars: Vec<char> = line.chars().collect();
    for i in 0..chars.len() {
        if !chars[i].eq_ignore_ascii_case(&'a') {
            continue;
        }
        if chars
            .get(i + 1)
            .is_none_or(|ch| !ch.eq_ignore_ascii_case(&'s'))
        {
            continue;
        }
        let before = i > 0 && super::is_name(chars[i - 1]);
        let after = chars.get(i + 2).is_some_and(|ch| super::is_name(*ch));
        if !before && !after {
            return true;
        }
    }
    false
}

fn best_covering<'a>(occs: &'a [Occ<'a>], col: usize) -> Option<Chosen<'a>> {
    let mut best: Option<&Occ<'a>> = None;
    for hit in occs {
        if !hit.covers(col) {
            continue;
        }
        let take = best.is_none_or(|cur| rank(hit, col) > rank(cur, col));
        if take {
            best = Some(hit);
        }
    }
    best.map(|hit| Chosen {
        text: hit.text,
        start: hit.start,
    })
}

fn rank(hit: &Occ<'_>, col: usize) -> (bool, usize, Kind) {
    (hit.strict(col), hit.end - hit.start, hit.kind)
}

/// Labels on `line`. A comment is empty. Each line is read at most twice.
fn line_occs<'a>(line: &str, prep: &Prep<'a>) -> Vec<Occ<'a>> {
    let Some(raw) = text::read(line, false) else {
        return Vec::new();
    };
    let plain = text::read(line, true);
    let mut found = Vec::new();
    collect(line, &raw, prep, &mut found);
    if plain.as_ref().is_some_and(|norm| norm.text != raw.text)
        && let Some(norm) = &plain
    {
        collect(line, norm, prep, &mut found);
    }
    found.sort_by_key(|hit| hit.label);
    let mut all = Vec::new();
    let mut found = found.into_iter().peekable();
    while let Some(label) = found.peek().map(|hit| hit.label) {
        let mut group = Vec::new();
        while found.peek().is_some_and(|hit| hit.label == label) {
            group.push(found.next().expect("peeked").occ);
        }
        let mut kept = suppress(group);
        for hit in &mut kept {
            attach(line, hit);
        }
        all.extend(kept);
    }
    drop_nested(all)
}

fn collect<'a>(line: &str, norm: &Norm, prep: &Prep<'a>, found: &mut Vec<Hit<'a>>) {
    let mut probes = prep.shorts.clone();
    for word in name_words(&norm.text) {
        let mut chars = 0;
        let mut byte = 0;
        for ch in word.chars() {
            byte += ch.len_utf8();
            chars += 1;
            if chars >= ANCHOR
                && let Some(ids) = prep.anchored.get(&word[..byte])
            {
                probes.extend_from_slice(ids);
            }
        }
    }
    probes.sort_unstable();
    probes.dedup();
    for probe_at in probes {
        let probe = &prep.probes[probe_at];
        push_needle(line, norm, prep, probe, found);
    }
    if let Some(labels) = prep.fragments.get(&norm.text) {
        for &label in labels {
            push_span(
                line,
                norm,
                prep,
                At {
                    label,
                    from: 0,
                    to: norm.text.len(),
                    kind: Kind::Fragment,
                },
                found,
            );
        }
    }
}

/// Where a needle sat in the normalized line.
struct At {
    label: usize,
    from: usize,
    to: usize,
    kind: Kind,
}

fn push_needle<'a>(
    line: &str,
    norm: &Norm,
    prep: &Prep<'a>,
    probe: &Probe,
    found: &mut Vec<Hit<'a>>,
) {
    for (from, to) in text::locate_in(&norm.text, &probe.needle, probe.kind == Kind::Ellipsis) {
        push_span(
            line,
            norm,
            prep,
            At {
                label: probe.label,
                from,
                to,
                kind: probe.kind,
            },
            found,
        );
    }
}

fn push_span<'a>(line: &str, norm: &Norm, prep: &Prep<'a>, at: At, found: &mut Vec<Hit<'a>>) {
    let Some((start, end)) = norm.map(at.from, at.to) else {
        return;
    };
    if start >= end {
        return;
    }
    let (start, end) = hug(line, start, end);
    found.push(Hit {
        label: at.label,
        occ: Occ {
            text: prep.labels[at.label],
            start,
            end,
            also: Vec::new(),
            alias: None,
            kind: at.kind,
        },
    });
}

fn name_words(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        let mut skip = 0;
        for ch in rest.chars() {
            if super::is_name(ch) {
                break;
            }
            skip += ch.len_utf8();
        }
        if skip >= rest.len() {
            return None;
        }
        rest = &rest[skip..];
        let mut take = 0;
        for ch in rest.chars() {
            if !super::is_name(ch) {
                break;
            }
            take += ch.len_utf8();
        }
        let word = &rest[..take];
        rest = &rest[take..];
        (!word.is_empty()).then_some(word)
    })
}

fn anchor_runs(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut run = String::new();
    let mut chars = 0;
    for ch in text.chars() {
        if super::is_name(ch) {
            run.push(ch);
            chars += 1;
            continue;
        }
        keep_run(&mut out, &mut run, &mut chars);
    }
    keep_run(&mut out, &mut run, &mut chars);
    out
}

fn keep_run(out: &mut Vec<String>, run: &mut String, chars: &mut usize) {
    if *chars >= ANCHOR {
        out.push(std::mem::take(run));
    } else {
        run.clear();
    }
    *chars = 0;
}

fn candidates(label: &str) -> Vec<(Kind, String)> {
    let mut out = vec![(Kind::Exact, label.to_owned())];
    if let Some(inner) = unwrap_layer(label)
        && inner != label
    {
        out.push((Kind::Unwrapped, inner));
    }
    if label.contains('«') || label.contains('»') {
        let spelled = label.replace('«', "<<").replace('»', ">>");
        if spelled != label {
            out.push((Kind::Exact, spelled));
        }
    }
    if let Some(stem) = ellipsis_stem(label) {
        out.push((Kind::Ellipsis, stem));
    }
    out
}

fn unwrap_layer(label: &str) -> Option<String> {
    let open = label.chars().next()?;
    let close = label.chars().next_back()?;
    let pair = matches!(
        (open, close),
        ('«', '»') | ('<', '>') | ('[', ']') | ('(', ')') | ('"', '"')
    );
    if !pair {
        return None;
    }
    let inner = label[open.len_utf8()..label.len() - close.len_utf8()].trim();
    (!inner.is_empty()).then(|| inner.to_owned())
}

fn ellipsis_stem(label: &str) -> Option<String> {
    let stem = label
        .strip_suffix('…')
        .or_else(|| label.strip_suffix("..."))?
        .trim_end();
    (stem.chars().count() >= 3).then(|| stem.to_owned())
}

fn fragments(label: &str) -> Vec<String> {
    let words: Vec<&str> = label.split_whitespace().collect();
    if words.len() < 2 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for n in (1..words.len()).rev() {
        for piece in [words[..n].join(" "), words[words.len() - n..].join(" ")] {
            if piece.chars().count() >= 3 && piece != label && !out.contains(&piece) {
                out.push(piece);
            }
        }
    }
    out
}

/// Drops a find that only repeats a stronger one of the same label.
fn suppress(mut found: Vec<Occ<'_>>) -> Vec<Occ<'_>> {
    found.sort_by(|a, b| {
        b.kind
            .cmp(&a.kind)
            .then((b.end - b.start).cmp(&(a.end - a.start)))
            .then(a.start.cmp(&b.start))
    });
    let mut kept = Vec::new();
    for hit in found {
        if kept.iter().any(|earlier| overlaps(earlier, &hit)) {
            continue;
        }
        kept.push(hit);
    }
    kept.sort_by_key(|hit| hit.start);
    kept
}

fn overlaps(a: &Occ<'_>, b: &Occ<'_>) -> bool {
    a.start < b.end && b.start < a.end
}

/// Drops a short label that only repeats part of a longer one.
fn drop_nested(all: Vec<Occ<'_>>) -> Vec<Occ<'_>> {
    let keep: Vec<bool> = all
        .iter()
        .map(|hit| {
            !all.iter().any(|other| {
                other.text != hit.text
                    && other.start <= hit.start
                    && hit.end <= other.end
                    && other.end - other.start > hit.end - hit.start
            })
        })
        .collect();
    let mut kept: Vec<_> = all
        .into_iter()
        .zip(keep)
        .filter_map(|(hit, keep)| keep.then_some(hit))
        .collect();
    kept.sort_by_key(|hit| hit.start);
    kept
}

/// The ident `as` gives this label, after it or before it.
fn attach(line: &str, hit: &mut Occ<'_>) {
    let named = alias_after(line, hit.end).or_else(|| alias_before(line, hit.start));
    let mut also = Vec::new();
    let mut alias = None;
    if let Some(range) = named {
        also.push(range);
        alias = Some(range);
    }
    let outside = |from: usize, to: usize| to <= hit.start || from >= hit.end;
    also.retain(|(from, to)| outside(*from, *to));
    if let Some((from, to)) = alias
        && !outside(from, to)
    {
        alias = None;
    }
    hit.also = also;
    hit.alias = alias;
}

fn alias_after(line: &str, end: usize) -> Option<(usize, usize)> {
    let (as_at, as_end) = token_at(line, skip_ws(line, end))?;
    if !line[as_at..as_end].eq_ignore_ascii_case("as") {
        return None;
    }
    token_at(line, skip_ws(line, as_end))
}

fn alias_before(line: &str, start: usize) -> Option<(usize, usize)> {
    let (as_at, _) = word_as(line, start)?;
    token_before(line, as_at)
}

fn word_as(line: &str, start: usize) -> Option<(usize, usize)> {
    let (from, to) = token_before(line, start)?;
    line[from..to]
        .eq_ignore_ascii_case("as")
        .then_some((from, to))
}

fn token_before(line: &str, start: usize) -> Option<(usize, usize)> {
    let end = skip_ws_back(line, start);
    if end == 0 || !super::is_name(line[..end].chars().next_back()?) {
        return None;
    }
    let from = super::name_start(line, end);
    (from < end).then_some((from, end))
}

fn token_at(line: &str, at: usize) -> Option<(usize, usize)> {
    if !super::is_name(line[at..].chars().next()?) {
        return None;
    }
    Some((at, super::name_end(line, at)))
}

/// The caret is on a use of an alias, on a line that does not draw the label.
fn alias_use<'a>(line: &str, col: usize, aliases: &[(String, &'a str)]) -> Option<Chosen<'a>> {
    let (from, to) = super::word_range(line, col)?;
    let name = &line[from..to];
    let text = aliases.iter().find(|(alias, _)| alias == name)?.1;
    Some(Chosen { text, start: from })
}

/// The caret sits in `{...}`, `(...)` or `<<...>>` that the drawing does not
/// spell. The name it belongs to is the nearest label with only space between.
fn nearest_bracket<'a>(line: &str, col: usize, occs: &'a [Occ<'a>]) -> Option<Chosen<'a>> {
    let (open, close) = group_around(line, col)?;
    let mut best: Option<(usize, &Occ<'a>)> = None;
    for hit in occs {
        let Some(dist) = beside(line, hit.start, hit.end, open, close) else {
            continue;
        };
        let take = match &best {
            None => true,
            Some((cur, held)) => {
                dist < *cur
                    || (dist == *cur
                        && (hit.end - hit.start, hit.kind) > (held.end - held.start, held.kind))
            }
        };
        if take {
            best = Some((dist, hit));
        }
    }
    best.map(|(_, hit)| Chosen {
        text: hit.text,
        start: hit.start,
    })
}

/// Bytes between a hit and a bracket group, when only space stands there.
fn beside(line: &str, start: usize, end: usize, open: usize, close: usize) -> Option<usize> {
    let between = if end <= open {
        line.get(end..open)?
    } else if start >= close {
        line.get(close..start)?
    } else {
        return None;
    };
    between
        .chars()
        .all(char::is_whitespace)
        .then_some(between.len())
}

/// The smallest `<<>>`, `{...}` or `(...)` that holds `col`.
fn group_around(line: &str, col: usize) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < line.len() {
        if line[i..].starts_with("<<") {
            if let Some(rel) = line[i + 2..].find(">>") {
                let end = i + 2 + rel + 2;
                if col >= i && col < end {
                    tighten(&mut best, i, end);
                }
            }
            i += 2;
            continue;
        }
        let open = line[i..].chars().next()?;
        let width = open.len_utf8();
        if let Some(close) = (open == '{')
            .then_some('}')
            .or((open == '(').then_some(')'))
            && let Some(len) = closer(&line[i + width..], open, close)
        {
            let end = i + width + len;
            if col >= i && col < end {
                tighten(&mut best, i, end);
            }
        }
        i += width;
    }
    best
}

fn closer(text: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 1;
    for (index, ch) in text.char_indices() {
        if ch == open {
            depth += 1;
        }
        if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(index + ch.len_utf8());
            }
        }
    }
    None
}

fn tighten(best: &mut Option<(usize, usize)>, open: usize, end: usize) {
    let replace = best.is_none_or(|(from, to)| end - open < to - from);
    if replace {
        *best = Some((open, end));
    }
}

/// Quotes, parentheses or brackets that hug a label, and a `:` or `;`
/// that only introduces or closes it, join the span.
fn hug(line: &str, mut start: usize, mut end: usize) -> (usize, usize) {
    if let (Some((from, open)), Some((_, close, after))) =
        (prev_char(line, start), next_char(line, end))
        && matches!(
            (open, close),
            ('"', '"') | ('(', ')') | ('[', ']') | ('«', '»')
        )
    {
        start = from;
        end = after;
    }
    if let Some((from, ch)) = prev_char(line, start)
        && ch == ':'
    {
        start = from;
    }
    if let Some((_, ch, after)) = next_char(line, end)
        && ch == ';'
    {
        end = after;
    }
    (start, end)
}

fn prev_char(line: &str, start: usize) -> Option<(usize, char)> {
    let end = skip_ws_back(line, start);
    if end == 0 {
        return None;
    }
    let ch = line[..end].chars().next_back()?;
    Some((end - ch.len_utf8(), ch))
}

fn next_char(line: &str, end: usize) -> Option<(usize, char, usize)> {
    let at = skip_ws(line, end);
    let ch = line[at..].chars().next()?;
    Some((at, ch, at + ch.len_utf8()))
}

fn skip_ws(line: &str, mut index: usize) -> usize {
    while let Some(ch) = line[index..].chars().next() {
        if !ch.is_whitespace() {
            break;
        }
        index += ch.len_utf8();
    }
    index
}

fn skip_ws_back(line: &str, mut index: usize) -> usize {
    while index > 0 {
        let ch = line[..index].chars().next_back().unwrap();
        if !ch.is_whitespace() {
            break;
        }
        index -= ch.len_utf8();
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `aimed` at the middle of the `which`th `at` on `row`.
    fn aim(
        source: &str,
        row: usize,
        at: &str,
        which: usize,
        texts: &[&str],
    ) -> Option<(String, usize)> {
        let line = source.split('\n').nth(row).unwrap();
        let mut from = 0;
        let mut col = 0;
        for index in 0..=which {
            let rel = line[from..]
                .find(at)
                .unwrap_or_else(|| panic!("{at} in {line}"));
            col = from + rel;
            if index == which {
                col += at.len() / 2;
                break;
            }
            from += rel + at.len();
        }
        aimed(source, row, col, texts.iter().copied())
    }

    #[test]
    fn the_caret_aims_at_the_phrase_it_stands_in() {
        let source = "\
participant \"Web app\" as Web
User -> Web : open the login page
alt password ok
state \"Request (idle)\" as Request
state Pick <<choice>>
:Merge the last change;
if (all green?) then (yes)
component Billing <<service>>
interface Sellable
block Filter \"Low pass (dc)\" {
[Web] as Front
render **bold** and //italic//
A -> B : a rather long message text
";
        let texts = [
            "Web app",
            "User",
            "open the login page",
            "[password ok]",
            "ok",
            "alt",
            "Request (idle)",
            "Pick",
            "Merge the last change",
            "all green?",
            "yes",
            "«service»",
            "Billing",
            "«interface»",
            "Sellable",
            "Low pass (dc)",
            "render bold and italic",
            "a rather long message te…",
        ];
        assert_eq!(
            aim(source, 0, "Web", 0, &texts),
            Some(("Web app".into(), 0))
        );
        assert_eq!(
            aim(source, 0, "app", 0, &texts),
            Some(("Web app".into(), 0))
        );
        // The alias names the same box, and it is the first copy.
        assert_eq!(
            aim(source, 0, "Web", 1, &texts),
            Some(("Web app".into(), 0))
        );
        assert_eq!(aim(source, 0, "participant", 0, &texts), None);
        assert_eq!(
            aim(source, 1, "open", 0, &texts),
            Some(("open the login page".into(), 0))
        );
        assert_eq!(
            aim(source, 1, "page", 0, &texts),
            Some(("open the login page".into(), 0))
        );
        // A later line calls the participant by its alias.
        assert_eq!(
            aim(source, 1, "Web", 0, &texts),
            Some(("Web app".into(), 1))
        );
        assert_eq!(
            aim(source, 2, "password", 0, &texts),
            Some(("[password ok]".into(), 0))
        );
        assert_eq!(
            aim(source, 2, "ok", 0, &texts),
            Some(("[password ok]".into(), 0))
        );
        assert_eq!(aim(source, 2, "alt", 0, &texts), Some(("alt".into(), 0)));
        assert_eq!(
            aim(source, 3, "(", 0, &texts),
            Some(("Request (idle)".into(), 0))
        );
        assert_eq!(
            aim(source, 3, "idle", 0, &texts),
            Some(("Request (idle)".into(), 0))
        );
        assert_eq!(
            aim(source, 3, "Request", 1, &texts),
            Some(("Request (idle)".into(), 0))
        );
        assert_eq!(aim(source, 3, "state", 0, &texts), None);
        assert_eq!(
            aim(source, 4, "choice", 0, &texts),
            Some(("Pick".into(), 0))
        );
        assert_eq!(aim(source, 4, "Pick", 0, &texts), Some(("Pick".into(), 0)));
        assert_eq!(
            aim(source, 5, "Merge", 0, &texts),
            Some(("Merge the last change".into(), 0))
        );
        assert_eq!(
            aim(source, 5, ";", 0, &texts),
            Some(("Merge the last change".into(), 0))
        );
        assert_eq!(
            aim(source, 6, "green", 0, &texts),
            Some(("all green?".into(), 0))
        );
        assert_eq!(aim(source, 6, "yes", 0, &texts), Some(("yes".into(), 0)));
        assert_eq!(
            aim(source, 7, "service", 0, &texts),
            Some(("«service»".into(), 0))
        );
        assert_eq!(
            aim(source, 7, "<", 0, &texts),
            Some(("«service»".into(), 0))
        );
        assert_eq!(
            aim(source, 8, "interface", 0, &texts),
            Some(("«interface»".into(), 0))
        );
        assert_eq!(
            aim(source, 8, "Sellable", 0, &texts),
            Some(("Sellable".into(), 0))
        );
        assert_eq!(
            aim(source, 9, "dc", 0, &texts),
            Some(("Low pass (dc)".into(), 0))
        );
        // The block's own name is not the quoted label, and there is no `as`.
        assert_eq!(aim(source, 9, "Filter", 0, &texts), None);
        assert_eq!(
            aim("[Web] as Front", 0, "Front", 0, &["Web"]),
            Some(("Web".into(), 0))
        );
        assert_eq!(
            aim("[Web] as Front", 0, "Web", 0, &["Web"]),
            Some(("Web".into(), 0))
        );
        assert_eq!(
            aim(source, 11, "bold", 0, &texts),
            Some(("render bold and italic".into(), 0))
        );
        assert_eq!(
            aim(source, 12, "rather", 0, &texts),
            Some(("a rather long message te…".into(), 0))
        );
        assert_eq!(aim("' Web app\n", 0, "Web", 0, &texts), None);
        assert_eq!(aim("state Player {", 0, "{", 0, &["Player"]), None);
    }

    #[test]
    fn an_alias_on_a_later_line_aims_at_the_same_label() {
        let source = "state \"Request (idle)\" as Request\nRequest --> Pick : go\n";
        let texts = ["Request (idle)", "Pick", "go"];
        assert_eq!(
            aim(source, 1, "Request", 0, &texts),
            Some(("Request (idle)".into(), 1))
        );
        assert_eq!(aim(source, 1, "Pick", 0, &texts), Some(("Pick".into(), 0)));
        assert_eq!(aim(source, 1, "go", 0, &texts), Some(("go".into(), 0)));
        assert_eq!(aim(source, 1, "-->", 0, &texts), None);
    }

    #[test]
    fn two_names_on_one_line_are_two_copies() {
        let source = "A[Edit] --> B[Edit]\n  C[Edit] --> D\n";
        assert_eq!(
            aim(source, 0, "Edit", 0, &["Edit"]),
            Some(("Edit".into(), 0))
        );
        assert_eq!(
            aim(source, 0, "Edit", 1, &["Edit"]),
            Some(("Edit".into(), 1))
        );
        assert_eq!(
            aim(source, 1, "Edit", 0, &["Edit"]),
            Some(("Edit".into(), 2))
        );
        assert_eq!(aim(source, 0, "-->", 0, &["Edit"]), None);
    }

    #[test]
    fn a_note_split_over_lines_still_aims() {
        let source = "note over Gate\n  spanning\n  note\nend note\n";
        let texts = ["spanning note", "Gate"];
        assert_eq!(
            aim(source, 1, "spanning", 0, &texts),
            Some(("spanning note".into(), 0))
        );
        assert_eq!(
            aim(source, 2, "note", 0, &texts),
            Some(("spanning note".into(), 1))
        );
        assert_eq!(aim(source, 3, "note", 0, &texts), None);
        assert_eq!(aim(source, 0, "Gate", 0, &texts), Some(("Gate".into(), 0)));
    }

    #[test]
    fn a_modifier_in_braces_aims_at_its_member() {
        let source = "{static} + count : int\n{abstract} + name() : String\n";
        let texts = ["+ count : int", "+ name() : String"];
        assert_eq!(
            aim(source, 0, "static", 0, &texts),
            Some(("+ count : int".into(), 0))
        );
        assert_eq!(
            aim(source, 0, "count", 0, &texts),
            Some(("+ count : int".into(), 0))
        );
        assert_eq!(
            aim(source, 1, "abstract", 0, &texts),
            Some(("+ name() : String".into(), 0))
        );
        assert_eq!(
            aim(source, 1, "name", 0, &texts),
            Some(("+ name() : String".into(), 0))
        );
        // More than space between the group and the name, so it is not that name.
        assert_eq!(
            aim("{static} + count : int and (nope)", 0, "nope", 0, &texts),
            None
        );
        assert_eq!(
            aim(
                "state \"Getting ready\" as Ready <<phase>>",
                0,
                "phase",
                0,
                &["Getting ready"]
            ),
            None
        );
    }

    #[test]
    fn a_long_file_aims_at_one_label() {
        let mut labels = Vec::new();
        let mut lines = Vec::new();
        for i in 0..400 {
            let phrase = format!("phase {i} waits for the next message from the station");
            lines.push(format!("state \"{phrase}\" as P{i}"));
            lines.push(format!("P{i} --> P{} : go onward", (i + 1) % 400));
            labels.push(phrase);
        }
        let source = lines.join("\n");
        let mut texts: Vec<&str> = labels.iter().map(String::as_str).collect();
        texts.push("go onward");
        // Line 1 is the first use of P0. The declaration above it is copy 0.
        let use_line = 1;
        let line = source.split('\n').nth(use_line).unwrap();
        let started = std::time::Instant::now();
        let hit = aimed(
            &source,
            use_line,
            line.find("P0").unwrap(),
            texts.iter().copied(),
        );
        let took = started.elapsed();
        assert_eq!(hit, Some((labels[0].clone(), 1)));
        let go = aimed(
            &source,
            use_line,
            line.find("go").unwrap(),
            texts.iter().copied(),
        );
        assert_eq!(go, Some(("go onward".into(), 0)));
        assert!(
            took < std::time::Duration::from_secs(2),
            "aimed took {took:?}"
        );
    }
}
