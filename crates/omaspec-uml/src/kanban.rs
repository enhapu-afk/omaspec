//! Mermaid kanban boards.
//!
//! The source is read here: an item at the first level of indentation is
//! a column, every item written deeper under it one of its cards. A card
//! may say who it is assigned to, its ticket and its priority
//! (`@{ assigned: 'kim', ticket: 'OM-12', priority: 'High' }`).
//!
//! The columns stand beside one another, all as tall as the fullest. A
//! card's words are broken to its width; its ticket and its assignee
//! stand under them, and its priority is a bar at its left edge.

use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::{Text, text_at};
use crate::source::starts_with_ci;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Font};

const COLUMN_W: f32 = 230.0;
const COLUMN_GAP: f32 = 14.0;
const PAD: f32 = 10.0;
const CARD_PAD: f32 = 9.0;
const CARD_GAP: f32 = 8.0;
/// The bar that says a card's priority.
const BAR: f32 = 4.0;

/// A kanban board.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    pub columns: Vec<Column>,
    pub look: Look,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub label: String,
    pub line: usize,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub label: String,
    pub line: usize,
    pub assigned: Option<String>,
    pub ticket: Option<String>,
    pub priority: Option<Priority>,
}

/// A card's priority, as Mermaid names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    VeryHigh,
    High,
    Low,
    VeryLow,
}

/// Type, ink, page, and the colors of a column, a card and the four
/// priorities (very high, high, low, very low). A theme replaces them.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    /// Mermaid's `look: handDrawn`: shapes drawn by hand (docs/adr/0023).
    pub sketch: bool,
    pub label_size: f32,
    pub title_size: f32,
    pub ink: Color,
    pub background: Paint,
    pub column: Color,
    pub card: Color,
    pub border: Color,
    pub priorities: [Color; 4],
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: "sans-serif".into(),
            sketch: false,
            label_size: 14.0,
            title_size: 20.0,
            ink: Color::hex(0x333333),
            background: None,
            column: Color::hex(0xEEEEF4),
            card: Color::WHITE,
            border: Color::hex(0xB9B9C6),
            priorities: [0xD9363E, 0xE8912D, 0x3E7BFA, 0x8AB4F8].map(Color::hex),
        }
    }
}

/// The board `source` describes, or `None` when it is not a `kanban` or
/// it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a kanban board, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read(source) {
        Err(Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

/// A card measured: its words, what stands under them, how tall it is.
struct Laid {
    words: Block,
    ticket: Option<Block>,
    assigned: Option<Block>,
    height: f32,
}

/// The SVG. Words that the text engine cannot draw decline the board.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    let mut canvas = Canvas::new(&diagram.look.font_name, false);
    canvas.sketch(diagram.look.sketch);
    let family = canvas.family();
    let look = &diagram.look;
    let title = match &diagram.title {
        Some(title) => Some(
            text::block(
                &title.value,
                Font::new(look.title_size, look.ink).bold(),
                family,
            )
            .map_err(|reason| Declined::at(title.line, reason))?,
        ),
        None => None,
    };
    let top = title.as_ref().map_or(0.0, |block| block.height + 14.0);
    let on_column = on_fill(look.column);
    let on_card = on_fill(look.card);
    let inner = COLUMN_W - 2.0 * PAD;
    let small = look.label_size * 0.82;

    let mut laid = Vec::with_capacity(diagram.columns.len());
    for column in &diagram.columns {
        let head = text::wrapped(
            &column.label,
            Font::new(look.label_size, on_column).bold(),
            family,
            inner,
        )
        .map_err(|reason| Declined::at(column.line, reason))?;
        let mut cards = Vec::with_capacity(column.cards.len());
        for card in &column.cards {
            let at = |reason| Declined::at(card.line, reason);
            let room = inner - 2.0 * CARD_PAD - BAR;
            let words = text::wrapped(
                &card.label,
                Font::new(look.label_size, on_card),
                family,
                room,
            )
            .map_err(at)?;
            let dim = Font::new(small, on_card.mix(look.card, 0.35));
            let under = |words: &Option<String>| match words {
                Some(words) => text::block(words, dim, family).map(Some),
                None => Ok(None),
            };
            let ticket = under(&card.ticket).map_err(at)?;
            let assigned = under(&card.assigned).map_err(at)?;
            let foot = [&ticket, &assigned]
                .into_iter()
                .flatten()
                .map(|block| block.height + 6.0)
                .fold(0.0, f32::max);
            cards.push(Laid {
                height: words.height + foot + 2.0 * CARD_PAD,
                words,
                ticket,
                assigned,
            });
        }
        laid.push((head, cards));
    }
    let tallest = laid
        .iter()
        .map(|(head, cards)| {
            let stack: f32 = cards.iter().map(|card| card.height + CARD_GAP).sum();
            head.height + 2.0 * PAD + stack + PAD - CARD_GAP.min(stack)
        })
        .fold(0.0, f32::max);

    for (index, (column, (head, cards))) in diagram.columns.iter().zip(&laid).enumerate() {
        let x = index as f32 * (COLUMN_W + COLUMN_GAP);
        canvas.rect(
            Layer::Background,
            (x, top, COLUMN_W, tallest),
            8.0,
            Some(look.column),
            None,
        );
        canvas.text(Layer::Front, head, (x + PAD, top + PAD), Align::Left);
        let mut y = top + PAD + head.height + PAD;
        for (card, laid) in column.cards.iter().zip(cards) {
            let rect = (x + PAD, y, inner, laid.height);
            canvas.rect(
                Layer::Bars,
                rect,
                5.0,
                Some(look.card),
                Some(Stroke::new(look.border, 1.0)),
            );
            if let Some(priority) = card.priority {
                let color = look.priorities[priority as usize];
                let bar = (x + PAD, y + 3.0, BAR, laid.height - 6.0);
                canvas.rect(Layer::Bars, bar, 2.0, Some(color), None);
            }
            let left = x + PAD + BAR + CARD_PAD;
            canvas.text(Layer::Front, &laid.words, (left, y + CARD_PAD), Align::Left);
            let foot = y + CARD_PAD + laid.words.height + 6.0;
            if let Some(ticket) = &laid.ticket {
                canvas.text(Layer::Front, ticket, (left, foot), Align::Left);
            }
            if let Some(assigned) = &laid.assigned {
                let right = x + PAD + inner - CARD_PAD;
                canvas.text(Layer::Front, assigned, (right, foot), Align::Right);
            }
            y += laid.height + CARD_GAP;
        }
    }
    if let Some(block) = &title {
        let count = diagram.columns.len().max(1) as f32;
        let width = count * COLUMN_W + (count - 1.0) * COLUMN_GAP;
        canvas.text(Layer::Front, block, (width / 2.0, 0.0), Align::Center);
    }
    Ok(canvas.finish(10.0, look.background))
}

/// Black on a light fill, white on a dark one.
fn on_fill(fill: Color) -> Color {
    if fill.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

struct Reason {
    line: usize,
    what: &'static str,
}

enum Stop {
    /// Not a kanban board.
    Foreign,
    At(Reason),
}

fn stop(line: usize, what: &'static str) -> Stop {
    Stop::At(Reason { line, what })
}

fn read(source: &str) -> Result<Diagram, Stop> {
    let mut title = None;
    let mut front = None;
    let mut iter = source.lines().enumerate().peekable();
    if iter.peek().is_some_and(|(_, raw)| raw.trim() == "---") {
        iter.next();
        let mut kept = crate::source::Front::default();
        for (no, raw) in iter.by_ref() {
            let text = raw.trim();
            if text == "---" {
                break;
            }
            if text.is_empty() || text.starts_with('#') || kept.drawn(raw) {
                continue;
            }
            match text.split_once(':') {
                Some((key, value)) if key.trim().eq_ignore_ascii_case("title") => {
                    let value = value.trim().trim_matches(['"', '\'']);
                    if !value.is_empty() {
                        title = Some(text_at(value, no));
                    }
                }
                _ => {
                    front.get_or_insert(no);
                }
            }
        }
    }
    // The lines that say something, each with how deep it stands.
    let mut lines = iter.filter_map(|(no, raw)| {
        let said = match raw.find("%%") {
            Some(at) => &raw[..at],
            None => raw,
        };
        let text = said.trim();
        let depth = said.len() - said.trim_start().len();
        (!text.is_empty()).then_some((no, depth, text))
    });
    let Some((_, _, header)) = lines.next() else {
        return Err(Stop::Foreign);
    };
    if !starts_with_ci(header, "kanban") || header.len() != "kanban".len() {
        return Err(Stop::Foreign);
    }
    if let Some(line) = front {
        return Err(stop(line, "a front matter setting"));
    }
    let mut columns: Vec<Column> = Vec::new();
    let mut first = None;
    for (line, depth, text) in lines {
        if starts_with_ci(text, "accTitle") || starts_with_ci(text, "accDescr") {
            continue;
        }
        let (item, settings) = match text.split_once("@{") {
            Some((item, rest)) => {
                let inner = rest
                    .trim_end()
                    .strip_suffix('}')
                    .ok_or(stop(line, "settings over several lines"))?;
                (item.trim(), Some(inner))
            }
            None => (text, None),
        };
        let label = label_of(item).ok_or(stop(line, "a kanban item"))?;
        if depth <= *first.get_or_insert(depth) {
            if settings.is_some() {
                return Err(stop(line, "a column's settings"));
            }
            columns.push(Column {
                label,
                line,
                cards: Vec::new(),
            });
            continue;
        }
        let mut card = Card {
            label,
            line,
            assigned: None,
            ticket: None,
            priority: None,
        };
        for (key, value) in settings.map(pairs).unwrap_or_default() {
            match key {
                "assigned" => card.assigned = Some(value.to_owned()),
                "ticket" => card.ticket = Some(value.to_owned()),
                "priority" => {
                    card.priority = Some(match value {
                        "Very High" => Priority::VeryHigh,
                        "High" => Priority::High,
                        "Low" => Priority::Low,
                        "Very Low" => Priority::VeryLow,
                        _ => return Err(stop(line, "a priority")),
                    });
                }
                _ => return Err(stop(line, "a card's setting")),
            }
        }
        // The first item stands at the first level, so a column is there.
        if let Some(column) = columns.last_mut() {
            column.cards.push(card);
        }
    }
    Ok(Diagram {
        title,
        columns,
        look: Look::default(),
    })
}

/// What an item shows: the words in `id[words]` or `[words]`, or the
/// item itself when it has no brackets. `None` for another shape.
fn label_of(item: &str) -> Option<String> {
    let words = match (item.find('['), item.strip_suffix(']')) {
        (Some(open), Some(inner)) => &inner[open + 1..],
        (None, None) if !item.contains(['(', '{']) => item,
        _ => return None,
    };
    let words = words.trim().trim_matches('"').trim();
    (!words.is_empty()).then(|| {
        words
            .replace("<br/>", "\n")
            .replace("<br />", "\n")
            .replace("<br>", "\n")
    })
}

/// The `key: value` pairs between the braces of `@{ … }`, a value in
/// quotes or bare; a piece without a colon is left out.
fn pairs(inner: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = inner;
    while let Some((key, after)) = rest.split_once(':') {
        let after = after.trim_start();
        let (value, more) = match after.chars().next() {
            Some(quote @ ('\'' | '"')) => match after[1..].find(quote) {
                Some(end) => (&after[1..=end], &after[end + 2..]),
                None => (&after[1..], ""),
            },
            _ => match after.find(',') {
                Some(end) => (after[..end].trim(), &after[end..]),
                None => (after.trim(), ""),
            },
        };
        out.push((key.trim().trim_start_matches(',').trim(), value));
        rest = more;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = "---\ntitle: Sprint 4\n---\nkanban\n  Todo\n    [Write the guide]\n    docs[A card with words enough that they are broken over more than one line]\n  doing[In progress]\n    id6[Draw the board]@{ assigned: 'kim', ticket: OM-12, priority: 'Very High' }\n  [Done] %% nothing yet\n";

    #[test]
    fn columns_stand_at_the_first_level_and_cards_under_them() {
        let board = from_mermaid(BOARD).unwrap();
        assert_eq!(board.title.as_ref().unwrap().value, "Sprint 4");
        let told: Vec<(&str, usize)> = board
            .columns
            .iter()
            .map(|c| (c.label.as_str(), c.cards.len()))
            .collect();
        assert_eq!(told, [("Todo", 2), ("In progress", 1), ("Done", 0)]);
        assert_eq!(board.columns[0].cards[0].label, "Write the guide");
        let card = &board.columns[1].cards[0];
        assert_eq!(card.label, "Draw the board");
        assert_eq!(card.assigned.as_deref(), Some("kim"));
        assert_eq!(card.ticket.as_deref(), Some("OM-12"));
        assert_eq!(card.priority, Some(Priority::VeryHigh));
        assert_eq!((card.line, board.columns[2].line), (8, 9));
    }

    #[test]
    fn the_board_is_drawn_with_its_words_broken_to_the_card() {
        let board = from_mermaid(BOARD).unwrap();
        let svg = draw(&board).unwrap();
        for words in ["Sprint 4", "Todo", "In progress", "Done", "kim", "OM-12"] {
            assert!(svg.contains(&format!(">{words}<")), "{words}: {svg}");
        }
        let whole = "A card with words enough that they are broken over more than one line";
        assert!(!svg.contains(whole), "the long card is broken: {svg}");
        assert!(svg.contains(">A card with words"), "{svg}");
        assert!(svg.contains(&board.look.priorities[0].to_string()), "{svg}");
        // Three columns and three cards, and the bar of one priority.
        assert_eq!(svg.matches("<rect").count(), 3 + 3 + 1, "{svg}");
        assert!(
            draw(&from_mermaid("kanban\n").unwrap()).is_ok(),
            "an empty board"
        );
    }

    #[test]
    fn what_it_does_not_draw_is_refused_by_name() {
        for (source, line, what) in [
            (
                "kanban\n  Todo\n    a[x]@{ priority: 'Urgent' }\n",
                2,
                "a priority",
            ),
            (
                "kanban\n  Todo\n    a[x]@{ icon: 'bug' }\n",
                2,
                "a card's setting",
            ),
            (
                "kanban\n  Todo@{ assigned: 'kim' }\n",
                1,
                "a column's settings",
            ),
            (
                "kanban\n  Todo\n    a[x]@{ assigned: 'kim',\n",
                2,
                "settings over several lines",
            ),
            ("kanban\n  Todo\n    a(round)\n", 2, "a kanban item"),
            (
                "---\nconfig:\n  kanban:\n    ticketBaseUrl: 'https://x/#TICKET#'\n---\nkanban\n  Todo\n",
                2,
                "a front matter setting",
            ),
        ] {
            assert_eq!(mermaid_refusal(source), Some((line, what)), "{source}");
            assert!(from_mermaid(source).is_none());
        }
        assert_eq!(
            mermaid_refusal("kanban-beta\n  Todo\n"),
            None,
            "another diagram"
        );
        assert_eq!(mermaid_refusal(BOARD), None);
    }
}
