//! How a play's bar fits what it says into its width: the room inside it,
//! where the head stands, how many rows the options take and how much of
//! each label and target they keep, for a play and a width at once
//! ([`layout_of`]), so the bar keeps its size while the play runs.

use std::cell::RefCell;
use std::sync::{Arc, Weak};

use super::Offered;

/// The widest the bar gets, but for a machine whose options would keep
/// only their numbers in it.
pub(super) const WIDEST: f32 = 900.0;
/// Around the bar in the pane, across.
pub(super) const MARGIN: f32 = 18.0;
/// Inside the bar, across: its padding and its edge.
pub(super) const PADDING: f32 = 13.0;
/// Between the bar's items.
pub(super) const GAP: f32 = 8.0;
/// The row's height: a chip's, a line of text and its padding.
pub(super) const ROW: f32 = 26.0;
/// The height of a trace's second line.
pub(super) const NOTE: f32 = 18.0;
/// How wide a character is, as a share of its text's size. The bar's font
/// is the monospace one, 0.6 of its size wide (JetBrains Mono, DejaVu Sans
/// Mono, Fira Mono; Iosevka less); a little more, so a label is cut before
/// the row would run out.
pub(super) const CHAR: f32 = 0.605;
/// The fewest characters a label or a state is cut to.
pub(super) const SHORTEST: usize = 6;
/// The most rows of options the bar takes; past them the chips show their
/// numbers alone.
pub(super) const MOST_ROWS: usize = 3;
/// Between two rows of options.
pub(super) const ROW_GAP: f32 = 4.0;

/// The room inside the bar in a pane `pane` pixels wide.
pub(super) fn inside_of(pane: f32) -> f32 {
    (pane - 2.0 * MARGIN).min(WIDEST) - 2.0 * PADDING
}

/// How much higher than a row a bar laid out as `layout` is.
pub(super) fn extra_height_of(layout: Layout) -> f32 {
    rows_height(layout.rows + usize::from(layout.stacked)) - rows_height(1)
}

/// How high `rows` rows of options are, with the gaps between them.
pub(super) fn rows_height(rows: usize) -> f32 {
    rows as f32 * ROW + rows.saturating_sub(1) as f32 * ROW_GAP
}

/// What the bar laid out as `layout` says first of a machine at `at`: its
/// states, cut to a third of the room inside, or of a bar [`WIDEST`] wide
/// where it is wider (what more it has is the options').
pub(super) fn head_of(at: &str, layout: Layout) -> String {
    let room = if layout.stacked {
        // A row of its own, but for its mark and, where the bar has them,
        // the filler and the tools at its end by their signs, with a gap
        // before each.
        let tools = if layout.tools {
            tool_width("↶") + tool_width("⟲") + tool_width("✕") + 4.0 * GAP
        } else {
            0.0
        };
        layout.inside - width_of(2, 13.0) - tools
    } else {
        layout.inside.min(WIDEST - 2.0 * PADDING) / 3.0
    };
    format!("● {}", cut(at, (room / (13.0 * CHAR)) as usize))
}

/// What the bar of a machine says and where ([`machine_fit`]).
pub(super) struct Machine {
    pub(super) layout: Layout,
    pub(super) head: String,
    /// The room for the options, and how much more the tools give when
    /// they show only their signs.
    pub(super) room: f32,
    pub(super) signs: f32,
    pub(super) rows: usize,
}

/// What the bar of a play of `every` place says at `at` (none once the
/// machine has ended) with the tools `(back, again)` shown, in a pane
/// `pane` pixels wide, with its tools or without them (`tools`): its
/// layout, its head, the room for `options` and their rows.
pub(super) fn machine_fit(
    every: &Arc<Vec<Offered>>,
    pane: f32,
    tools: bool,
    at: Option<&str>,
    (back, again): (bool, bool),
    options: &[(String, String)],
) -> Machine {
    let layout = layout_of(every, pane, tools);
    let head = match at {
        Some(at) => head_of(at, layout),
        None => "● ended".to_owned(),
    };
    let (room, signs) = room_for(&head, layout, back, again);
    let rows = layout.rows.max(rows_needed(options, room, signs).1);
    Machine {
        layout,
        head,
        room,
        signs,
        rows,
    }
}

/// The room for a machine's options in `layout` beside `head` and the
/// tools shown, where it has them, and how much more the tools give when
/// they show only their signs: the whole bar below a head in a row of its
/// own.
pub(super) fn room_for(head: &str, layout: Layout, back: bool, again: bool) -> (f32, f32) {
    if layout.stacked {
        (layout.inside, 0.0)
    } else {
        room_beside(head, layout.tools.then_some((back, again)), layout.inside)
    }
}

/// The room left for a machine's options beside `head` in a bar `inside`
/// pixels wide inside, and how much more the tools give when they show
/// only their signs. `tools` says which of back and again are shown
/// beside the close; none where the bar has no tools, as a film's.
pub(super) fn room_beside(head: &str, tools: Option<(bool, bool)>, inside: f32) -> (f32, f32) {
    let mut used = 0.0;
    let mut signs = 0.0;
    if let Some((back, again)) = tools {
        // The close, and the gap before it.
        used += tool_width("✕") + GAP;
        for (shown, label, sign) in [(back, "↶ back", "↶"), (again, "⟲ again", "⟲")] {
            if shown {
                used += tool_width(label) + GAP;
                signs += tool_width(label) - tool_width(sign);
            }
        }
    }
    used += width_of(head.chars().count(), 13.0) + GAP;
    (inside - used, signs)
}

/// How the bar of a machine's play is laid out, for the play and a width
/// ([`layout_of`]): the room inside, the rows of options, whether the
/// head stands in a row of its own above them, the tools beside it, and
/// whether it has its tools (a film's has none: nothing there is pressed).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Layout {
    pub(super) inside: f32,
    pub(super) rows: usize,
    pub(super) stacked: bool,
    pub(super) tools: bool,
}

/// The layout of the bar of a play whose machine can be in each of `every`
/// place, in a pane `pane` pixels wide, with both tools shown as after a
/// first step where it has its tools (`tools`): as wide as [`WIDEST`] lets it where every place keeps its
/// labels there, else as wide as the pane; as many rows as the place that
/// needs most. Where the head beside the options would still cut their
/// labels or itself, as in a narrow pane, it stands in a row of its own,
/// should that show more at the place that shows least (REQ-045). It
/// depends on the play and the width alone, so it is worked out once for
/// each and kept.
pub(super) fn layout_of(every: &Arc<Vec<Offered>>, pane: f32, tools: bool) -> Layout {
    // Each play by a weak hold on its places, which keeps their address
    // from going to another play's; a few, so the editor's play and that
    // of a diagram in its own window keep theirs.
    type Kept = Vec<(Weak<Vec<Offered>>, u32, Layout)>;
    thread_local! {
        static KEPT: RefCell<Kept> = const { RefCell::new(Vec::new()) };
    }
    let width = pane.to_bits();
    let kept = KEPT.with_borrow(|kept| {
        kept.iter()
            .find(|(play, at, layout)| {
                *at == width && layout.tools == tools && play.ptr_eq(&Arc::downgrade(every))
            })
            .map(|&(_, _, layout)| layout)
    });
    if let Some(layout) = kept {
        return layout;
    }
    // Whether every place keeps its labels in a bar laid out so, and the
    // most rows a place needs there.
    let needs = |inside: f32, stacked: bool| {
        every
            .iter()
            .fold((true, 1), |(labels, most), (at, options)| {
                let layout = Layout {
                    inside,
                    rows: 1,
                    stacked,
                    tools,
                };
                let (room, signs) = room_for(&head_of(at, layout), layout, true, true);
                let (kept, rows) = rows_needed(options, room, signs);
                (labels && kept, most.max(rows))
            })
    };
    let narrow = inside_of(pane);
    let wide = pane - 2.0 * (MARGIN + PADDING);
    let (labels, rows) = needs(narrow, false);
    let beside = if labels || wide <= narrow {
        Layout {
            inside: narrow,
            rows,
            stacked: false,
            tools,
        }
    } else {
        Layout {
            inside: wide,
            rows: needs(wide, false).1,
            stacked: false,
            tools,
        }
    };
    // The labels cut, or the head: a row of its own may show more. Where
    // only where an option leads is cut, the bar stays one block.
    let shown = shown_by(every, beside);
    let layout = if shown.0 < Shown::Targets || shown.2 {
        let above = Layout {
            inside: beside.inside,
            rows: needs(beside.inside, true).1,
            stacked: true,
            tools,
        };
        let more = shown_by(every, above);
        if (more.0, more.1) > (shown.0, shown.1) {
            above
        } else {
            beside
        }
    } else {
        beside
    };
    KEPT.with_borrow_mut(|kept| {
        kept.retain(|(play, _, _)| play.strong_count() > 0);
        if kept.len() >= KEPT_PLAYS {
            kept.remove(0);
        }
        kept.push((Arc::downgrade(every), width, layout));
    });
    layout
}

/// What `layout` shows of `every` place: how much of the options at the
/// place that shows least, how many letters in all (the options' and the
/// heads'), and whether a head is cut.
pub(super) fn shown_by(every: &[Offered], layout: Layout) -> (Shown, usize, bool) {
    every.iter().fold(
        (Shown::Whole, 0, false),
        |(least, letters, cut_head), (at, options)| {
            let head = head_of(at, layout);
            let (room, signs) = room_for(&head, layout, true, true);
            let fit = arranged(options, room, signs, layout.rows);
            let words: usize = fit
                .options
                .iter()
                .map(|(label, to)| {
                    label.chars().count() + to.as_ref().map_or(0, |t| t.chars().count())
                })
                .sum();
            (
                least.min(fit.shown),
                letters + words + head.chars().count(),
                cut_head || head.ends_with('…'),
            )
        },
    )
}

/// How many plays and widths [`layout_of`] keeps.
pub(super) const KEPT_PLAYS: usize = 4;

/// The fewest rows, up to [`MOST_ROWS`], in which `options` keep their
/// labels, if cut, and whether they do; where not even the most do, the
/// fewest in which their numbers stand, none clipped; one where even
/// those do not, as in a narrow pane, where more rows would cover the
/// diagram and show hardly more (the keys 1 to 9 take them all).
pub(super) fn rows_needed(options: &[(String, String)], room: f32, signs: f32) -> (bool, usize) {
    let shown: Vec<Shown> = (1..=MOST_ROWS)
        .map(|rows| fitted(options, room, signs, rows).shown)
        .collect();
    let fewest = |enough: fn(Shown) -> bool| shown.iter().position(|&s| enough(s)).map(|at| at + 1);
    match fewest(|s| s > Shown::Numbers) {
        Some(rows) => (true, rows),
        None => (false, fewest(|s| s == Shown::Numbers).unwrap_or(1)),
    }
}

/// The options in at most `rows` rows: in as many as show the most of
/// them, the fewest of those.
pub(super) fn arranged(options: &[(String, String)], room: f32, signs: f32, rows: usize) -> Fit {
    // How much a fit shows: how far it went, then the letters it keeps.
    let shows = |fit: &Fit| {
        let letters: usize = fit
            .options
            .iter()
            .map(|(label, to)| {
                label.chars().count() + to.as_ref().map_or(0, |to| to.chars().count())
            })
            .sum();
        (fit.shown, letters)
    };
    let mut best = fitted(options, room, signs, 1);
    for rows in 2..=rows {
        let fit = fitted(options, room, signs, rows);
        if shows(&fit) > shows(&best) {
            best = fit;
        }
    }
    best
}

/// How wide `chars` characters of text `size` pixels high are.
pub(super) fn width_of(chars: usize, size: f32) -> f32 {
    chars as f32 * size * CHAR
}

/// How wide a tool of the bar is with `label`.
pub(super) fn tool_width(label: &str) -> f32 {
    16.0 + width_of(label.chars().count(), 13.0)
}

/// How wide the chip of option `number` is, with a label and a target of
/// these lengths (one of none is not shown).
pub(super) fn chip_width(number: usize, label: usize, to: usize) -> f32 {
    let badge = 12.0 + width_of(number.to_string().len(), 11.0);
    let shown = |chars: usize, size: f32, extra: usize| {
        if chars == 0 {
            0.0
        } else {
            7.0 + width_of(chars + extra, size)
        }
    };
    20.0 + badge + shown(label, 13.0, 0) + shown(to, 12.0, 2)
}

/// How wide a row of chips is, each given by the lengths of its label and
/// its target, with the gaps between them; the first is option `first`
/// (from 0), whose number its badge shows one higher.
pub(super) fn chips_width(first: usize, chips: impl Iterator<Item = (usize, usize)>) -> f32 {
    let all: f32 = chips
        .enumerate()
        .map(|(i, (label, to))| GAP + chip_width(first + i + 1, label, to))
        .sum();
    (all - GAP).max(0.0)
}

/// `text` in at most `most` characters, cut with `…`.
pub(super) fn cut(text: &str, most: usize) -> String {
    let most = most.max(SHORTEST);
    if text.chars().count() <= most {
        return text.to_owned();
    }
    let kept: String = text.chars().take(most - 1).collect();
    format!("{}…", kept.trim_end())
}

/// `text` in at most `lines` lines of `most` letters, broken between
/// words; the last cut where more is left.
pub(super) fn wrapped(text: &str, most: usize, lines: usize) -> String {
    let most = most.max(SHORTEST);
    let mut out: Vec<String> = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() && out.len() + 1 < lines {
        if rest.chars().count() <= most {
            break;
        }
        // The last space within the line, else the line cut where it is full.
        let end = rest
            .char_indices()
            .take(most + 1)
            .filter(|(_, c)| *c == ' ')
            .map(|(i, _)| i)
            .last()
            .filter(|&i| i > 0)
            .unwrap_or_else(|| rest.char_indices().nth(most).map_or(rest.len(), |(i, _)| i));
        out.push(rest[..end].trim_end().to_owned());
        rest = rest[end..].trim_start();
    }
    if !rest.is_empty() {
        out.push(cut(rest, most));
    }
    out.join("\n")
}

/// The options as their chips show them, each a label and perhaps where it
/// leads; whether the tools beside them show only their signs; how many
/// stand in a row; and how much of them is shown.
#[derive(Debug, PartialEq)]
pub(super) struct Fit {
    pub(super) options: Vec<(String, Option<String>)>,
    pub(super) signs: bool,
    pub(super) per_row: usize,
    pub(super) shown: Shown,
}

/// How much of its options a [`Fit`] shows, least first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Shown {
    /// Their numbers, some clipped at the bar's end.
    Clipped,
    /// Their numbers alone.
    Numbers,
    /// Their labels, some cut, without where they lead.
    Labels,
    /// Their labels whole; where they lead cut or left out.
    Targets,
    Whole,
}

/// The options in `rows` rows of `room` pixels, whole where they fit. Else
/// what is clicked stays whole longest: the tools show only their signs
/// (which gives `signs` pixels more), the targets are cut, longest first,
/// then left out, and only then are the labels cut, longest first; where
/// even the shortest do not fit, the chips show their numbers alone (the
/// keys 1 to 9 take them). What still does not fit is clipped at the bar's
/// end ([`Shown::Clipped`]).
pub(super) fn fitted(options: &[(String, String)], room: f32, signs: f32, rows: usize) -> Fit {
    let per_row = options.len().div_ceil(rows.max(1)).max(1);
    let count = |s: &str| s.chars().count();
    let width = |label_most: usize, to_most: Option<usize>| -> f32 {
        options
            .chunks(per_row)
            .enumerate()
            .map(|(row, line)| {
                chips_width(
                    row * per_row,
                    line.iter().map(|(label, to)| {
                        let to = to_most.map_or(0, |most| count(to).min(most));
                        (count(label).min(label_most), to)
                    }),
                )
            })
            .fold(0.0, f32::max)
    };
    let shown = |label_most: usize, to_most: Option<usize>, signs: bool, shown: Shown| Fit {
        options: options
            .iter()
            .map(|(label, to)| (cut(label, label_most), to_most.map(|most| cut(to, most))))
            .collect(),
        signs,
        per_row,
        shown,
    };
    let longest = |of: fn(&(String, String)) -> &String| {
        options
            .iter()
            .map(|o| count(of(o)))
            .max()
            .unwrap_or(0)
            .max(SHORTEST)
    };
    let (labels, targets) = (longest(|o| &o.0), longest(|o| &o.1));
    if width(labels, Some(targets)) <= room {
        return shown(labels, Some(targets), false, Shown::Whole);
    }
    let room = room + signs;
    for most in (SHORTEST..=targets).rev() {
        if width(labels, Some(most)) <= room {
            let whole = if most == targets {
                Shown::Whole
            } else {
                Shown::Targets
            };
            return shown(labels, Some(most), true, whole);
        }
    }
    for most in (SHORTEST..=labels).rev() {
        if width(most, None) <= room {
            let whole = if most == labels {
                Shown::Targets
            } else {
                Shown::Labels
            };
            return shown(most, None, true, whole);
        }
    }
    Fit {
        options: options.iter().map(|_| (String::new(), None)).collect(),
        signs: true,
        per_row,
        shown: if width(0, None) <= room {
            Shown::Numbers
        } else {
            Shown::Clipped
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(of: &[(&str, &str)]) -> Vec<(String, String)> {
        of.iter()
            .map(|(label, to)| ((*label).to_owned(), (*to).to_owned()))
            .collect()
    }

    fn width(fitted: &[(String, Option<String>)]) -> f32 {
        chips_width(
            0,
            fitted.iter().map(|(label, to)| {
                (
                    label.chars().count(),
                    to.as_ref().map_or(0, |t| t.chars().count()),
                )
            }),
        )
    }

    #[test]
    fn options_that_fit_are_shown_whole() {
        let of = options(&[("send failed", "Retry"), ("still down", "Still down")]);
        let fit = fitted(&of, 800.0, 90.0, 1);
        assert_eq!(
            fit,
            Fit {
                options: vec![
                    ("send failed".to_owned(), Some("Retry".to_owned())),
                    ("still down".to_owned(), Some("Still down".to_owned())),
                ],
                signs: false,
                per_row: 2,
                shown: Shown::Whole,
            }
        );
    }

    /// Where the options do not fit, the tools show their signs and the
    /// targets are cut before a label is.
    #[test]
    fn the_targets_give_way_before_the_labels() {
        let of = options(&[
            ("servers unexpectedly unavailable", "Degraded"),
            ("send failed", "Retry"),
        ]);
        let whole = width(&fitted(&of, 2000.0, 0.0, 1).options);
        // A little short: the longer target is cut.
        let fit = fitted(&of, whole - 10.0, 0.0, 1);
        assert!(fit.signs);
        assert_eq!(
            fit.options,
            [
                (
                    "servers unexpectedly unavailable".to_owned(),
                    Some("Degra…".to_owned())
                ),
                ("send failed".to_owned(), Some("Retry".to_owned())),
            ]
        );
        // Shorter: the targets are left out, the labels still whole.
        let fit = fitted(&of, whole - 30.0, 0.0, 1);
        assert_eq!(
            fit.options,
            [
                ("servers unexpectedly unavailable".to_owned(), None),
                ("send failed".to_owned(), None),
            ]
        );
        // Shorter still: the longest label is cut, the short one whole.
        let fit = fitted(&of, whole - 200.0, 0.0, 1);
        assert!(fit.options[0].0.ends_with('…'), "{fit:?}");
        assert_eq!(fit.options[1], ("send failed".to_owned(), None));
        assert!(width(&fit.options) <= whole - 200.0, "{fit:?}");
    }

    /// Grok's three options in a pane 800 pixels wide, beside the state and
    /// the tools: on one line, the short label whole, the long ones cut.
    #[test]
    fn long_options_are_cut_to_the_room_longest_first() {
        let of = options(&[
            ("restarts / reroute", "Fix attempt"),
            ("early signs of stabilisation", "Stabilizing"),
            ("send failed", "Retry"),
        ]);
        let room = 800.0 - 2.0 * MARGIN - 2.0 * PADDING - 330.0;
        let fit = fitted(&of, room, 90.0, 1);
        assert!(fit.signs);
        assert!(width(&fit.options) <= room + 90.0, "{fit:?}");
        assert!(fit.options.iter().all(|(_, to)| to.is_none()), "{fit:?}");
        assert!(fit.options[1].0.ends_with('…'), "{fit:?}");
        assert_eq!(fit.options[2].0, "send failed");
        // As long as the room allows: a little less, and it is cut more.
        let less = fitted(&of, width(&fit.options) - 90.0 - 1.0, 90.0, 1);
        assert_ne!(less, fit);
    }

    #[test]
    fn options_too_many_for_the_room_show_their_numbers() {
        let of = options(&[
            ("restarts and reroute", "Fix attempt"),
            ("early signs of stabilisation", "Stabilizing"),
            ("intermittent issues", "Degraded"),
            ("send failed", "Retry"),
        ]);
        // Room for the shortest labels: cut to them.
        let shortest = chips_width(0, (0..4).map(|_| (SHORTEST, 0)));
        let fit = fitted(&of, shortest, 0.0, 1);
        assert!(
            fit.options
                .iter()
                .all(|(label, to)| to.is_none() && (5..=SHORTEST).contains(&label.chars().count())),
            "{fit:?}"
        );
        // Less: the numbers alone, every option still there.
        let fit = fitted(&of, shortest - 1.0, 0.0, 1);
        assert_eq!(fit.options, vec![(String::new(), None); 4]);
        assert!(chips_width(0, (0..4).map(|_| (0, 0))) < shortest / 2.0);
    }

    /// REQ-045, `never.mmd` 800 pixels wide: in `Unstable`, after a step,
    /// six options beside back and again left their numbers alone. They
    /// take a second row, whole, and the bar has it in every state.
    #[test]
    fn options_that_would_show_their_numbers_take_a_second_row() {
        let unstable = options(&[
            ("try 1", "Attempt 1"),
            ("try 2", "Attempt 2"),
            ("try 3", "Attempt 3"),
            ("try 4", "Attempt 4"),
            ("try 5", "Attempt 5"),
            ("Rust 1.100", "Stable"),
        ]);
        let inside = 800.0 - 2.0 * MARGIN - 2.0 * PADDING;
        let beside = Layout {
            inside,
            rows: 1,
            stacked: false,
            tools: true,
        };
        let head = head_of("Unstable", beside);
        let (room, signs) = room_beside(&head, Some((true, true)), inside);
        assert_eq!(fitted(&unstable, room, signs, 1).shown, Shown::Numbers);
        assert_eq!(rows_needed(&unstable, room, signs), (true, 2));
        let fit = arranged(&unstable, room, signs, 2);
        assert_eq!(fit.per_row, 3);
        assert!(fit.shown >= Shown::Targets, "{fit:?}");
        assert_eq!(fit.options[5].0, "Rust 1.100");
        // At the start, without the tools, they fit in one; the bar keeps
        // the second row all the same.
        let (first, signs_first) = room_beside(&head, Some((false, false)), inside);
        assert!(fitted(&unstable, first, signs_first, 1).shown > Shown::Numbers);
        let reverted = options(&[("reverted", "Unstable")]);
        let attempt = ("Attempt 1".to_owned(), reverted.clone());
        let every = Arc::new(vec![("Unstable".to_owned(), unstable), attempt.clone()]);
        let two = Layout { rows: 2, ..beside };
        assert_eq!(layout_of(&every, 800.0, true), two);
        // Kept for the play and the width; another play counts its own.
        assert_eq!(layout_of(&every, 800.0, true), two);
        // A state with one option in a bar of two rows shows it in one.
        assert_eq!(arranged(&reverted, room, signs, 2).per_row, 1);
        assert_eq!(
            layout_of(&Arc::new(vec![attempt]), 800.0, true),
            Layout { rows: 1, ..two }
        );
        assert_eq!(rows_height(2), 2.0 * ROW + ROW_GAP);
    }

    /// A film's bar has no tools: nothing in it is pressed. The six options
    /// of `Unstable` that beside back, again and the close took a second
    /// row keep their labels in one, the head beside them as before.
    #[test]
    fn a_film_s_bar_gives_its_tools_room_to_the_options() {
        let unstable = options(&[
            ("try 1", "Attempt 1"),
            ("try 2", "Attempt 2"),
            ("try 3", "Attempt 3"),
            ("try 4", "Attempt 4"),
            ("try 5", "Attempt 5"),
            ("Rust 1.100", "Stable"),
        ]);
        let every = Arc::new(vec![("Unstable".to_owned(), unstable.clone())]);
        let shown = layout_of(&every, 800.0, true);
        let filmed = layout_of(&every, 800.0, false);
        assert_eq!((shown.rows, filmed.rows), (2, 1));
        assert!(!filmed.tools && !filmed.stacked, "{filmed:?}");
        let head = head_of("Unstable", filmed);
        assert_eq!(head, head_of("Unstable", shown));
        let (room, signs) = room_for(&head, filmed, true, true);
        let tools = tool_width("↶ back") + tool_width("⟲ again") + tool_width("✕") + 3.0 * GAP;
        assert_eq!(room, room_for(&head, shown, true, true).0 + tools);
        assert_eq!(signs, 0.0);
        assert!(fitted(&unstable, room, signs, 1).shown > Shown::Numbers);
        // A head in a row of its own, in a narrow bar, has the whole row
        // but its mark.
        let above = Layout {
            inside: inside_of(400.0),
            stacked: true,
            ..filmed
        };
        let at = "Besieging, WashedUp, PoseidonAway, Daydreaming, AtTheLoom";
        let tooled = Layout {
            tools: true,
            ..above
        };
        let letters = |layout| head_of(at, layout).chars().count();
        assert!(
            letters(above) > letters(tooled) + 10,
            "{}",
            head_of(at, above)
        );
    }

    /// A second row the bar has is taken where it shows more of the
    /// labels; and a badge of two digits is measured as two.
    #[test]
    fn rows_are_taken_only_where_they_show_more() {
        let long = options(&[
            ("the first of four rather long events", "One"),
            ("the second of four rather long events", "Two"),
            ("the third of four rather long events", "Three"),
            ("the fourth of four rather long events", "Four"),
        ]);
        let room = 520.0;
        let one = fitted(&long, room, 0.0, 1);
        let two = arranged(&long, room, 0.0, 2);
        assert_eq!(one.shown, Shown::Labels);
        assert_eq!(two.per_row, 2);
        let letters = |fit: &Fit| {
            fit.options
                .iter()
                .map(|(l, _)| l.chars().count())
                .sum::<usize>()
        };
        assert!(letters(&two) > letters(&one), "{two:?}");
        assert!(chips_width(9, [(5, 0)].into_iter()) > chips_width(0, [(5, 0)].into_iter()));
    }

    /// REQ-045, the Odyssey of 3,000 lines 1,400 pixels wide: seventeen
    /// options of regions side by side kept only their numbers in a bar
    /// 900 wide, and those in one row, clipped after the ninth. The bar is
    /// as wide as the pane where its options need it, and where beside the
    /// head they would still be cut, the head stands in a row of its own,
    /// whole, the options across the bar below it, their labels longer.
    #[test]
    fn many_options_side_by_side_widen_the_bar_and_keep_their_labels() {
        let many: Vec<(String, String)> = (1..=17)
            .map(|i| {
                (
                    format!("event {i} of a region that runs beside the others"),
                    format!("State{i}"),
                )
            })
            .collect();
        let at = "Besieging, WashedUp, PoseidonAway, Daydreaming, AtTheLoom, Waiting";
        let every = Arc::new(vec![(at.to_owned(), many.clone())]);
        let narrow = inside_of(1400.0);
        assert_eq!(narrow, WIDEST - 2.0 * PADDING);
        let beside = Layout {
            inside: narrow,
            rows: 1,
            stacked: false,
            tools: true,
        };
        let (room, signs) = room_beside(&head_of(at, beside), Some((true, true)), narrow);
        assert!(!rows_needed(&many, room, signs).0);
        let layout = layout_of(&every, 1400.0, true);
        assert_eq!(layout.inside, 1400.0 - 2.0 * (MARGIN + PADDING));
        assert!(layout.stacked && layout.rows <= MOST_ROWS, "{layout:?}");
        let head = head_of(at, layout);
        assert_eq!(head, format!("● {at}"));
        let (room, signs) = room_for(&head, layout, true, true);
        let fit = arranged(&many, room, signs, layout.rows);
        assert!(fit.shown >= Shown::Labels, "{fit:?}");
        let beside = Layout {
            stacked: false,
            ..layout
        };
        let side = shown_by(&every, beside);
        assert!(shown_by(&every, layout).1 > side.1);
        // Without the room for even that, the numbers take the rows they
        // need, none clipped: seventeen in three rows of six.
        assert_eq!(rows_needed(&many, 320.0, 0.0), (false, 3));
        assert_eq!(arranged(&many, 320.0, 0.0, 3).shown, Shown::Numbers);
        assert_eq!(fitted(&many, 320.0, 0.0, 1).shown, Shown::Clipped);
        // Where even the most rows clip them, the bar keeps one.
        assert_eq!(rows_needed(&many, 60.0, 0.0), (false, 1));
    }

    /// REQ-045 at 400 pixels: beside the head, two options of a starship
    /// were cut to eight letters and the head to fourteen, the ship's
    /// branch gone. The head stands in a row of its own, whole, the tools
    /// by their signs beside it, and the options keep nearly all of their
    /// labels below it.
    #[test]
    fn a_narrow_bar_puts_its_head_above_its_options() {
        let options = vec![
            ("GNC checks passed".to_owned(), "Ascent".to_owned()),
            ("26 Starlink deployed".to_owned(), "Coast".to_owned()),
        ];
        let at = "end of Booster, ShipCoast";
        let every = Arc::new(vec![(at.to_owned(), options.clone())]);
        let layout = layout_of(&every, 400.0, true);
        assert!(layout.stacked, "{layout:?}");
        let head = head_of(at, layout);
        assert_eq!(head, format!("● {at}"));
        let (room, signs) = room_for(&head, layout, true, true);
        let fit = arranged(&options, room, signs, layout.rows);
        // Fifteen letters of each where beside the head eight were left.
        assert!(fit.options[0].0.starts_with("GNC checks"), "{fit:?}");
        assert!(fit.options[1].0.starts_with("26 Starlink"), "{fit:?}");
        assert!(extra_height_of(layout) > 0.0);
    }

    #[test]
    fn a_cut_keeps_its_length_and_ends_in_an_ellipsis() {
        assert_eq!(cut("early signs of stabilisation", 12), "early signs…");
        assert_eq!(cut("send failed", 12), "send failed");
        // Never shorter than the shortest.
        assert_eq!(cut("restarts and reroute", 2), "resta…");
        assert_eq!(cut("Degraded, Send", 40), "Degraded, Send");
    }

    #[test]
    fn a_long_note_breaks_between_words_into_its_lines() {
        let note = "Door does not take open here: by now it is in Locked";
        assert_eq!(
            wrapped(note, 30, 2),
            "Door does not take open here:\nby now it is in Locked"
        );
        assert_eq!(wrapped(note, 30, 1), cut(note, 30));
        assert_eq!(wrapped("short", 30, 2), "short");
        let third = wrapped(&format!("{note} {note}"), 30, 2);
        assert_eq!(third.lines().count(), 2);
        assert!(third.ends_with('…'));
    }
}
