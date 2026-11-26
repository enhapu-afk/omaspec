//! Lays a story map out and paints it. The backbone runs across the top:
//! each activity a card over the columns of its steps, each step a card
//! over its column. Under it the releases are bands across the whole map,
//! their names down the left, every other one tinted; a story stands in
//! its step's column, in its release's band, as a note. A band is as high
//! as its fullest column.
//!
//! The words each line writes are tied to it: an activity's, a step's, a
//! story's, a release's name and goal. The map holds the backbone at the
//! top of a pane and the releases' names at its left as they scroll by.

use super::{Diagram, Step};
use crate::Declined;
use crate::annotations::Annotations;
use crate::graph::Rect;
use crate::map::{Band, Edge, Map};
use crate::own_skin::Skin;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Family, Font};

const MARGIN: f32 = 10.0;
/// Inside a card, around its words.
const PAD: f32 = 8.0;
/// Between two columns of one activity, and between two stories.
const GAP: f32 = 10.0;
/// Between two activities.
const APART: f32 = 24.0;
/// Between the activities and the steps.
const ROWS: f32 = 8.0;
/// Between the backbone and the first band.
const BELOW: f32 = 18.0;
/// Above and below the stories of a band.
const BAND_PAD: f32 = 12.0;
/// The widest a release's name and goal stand at the left.
const GUTTER: f32 = 150.0;
/// The words of a band without a release.
const UNPLANNED: &str = "not planned";

pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    draw_mapped(diagram).map(|(svg, _)| svg)
}

/// [`draw`], and the map: the words each line writes, the backbone a
/// pane keeps in sight at its top, the releases' names at its left.
pub fn draw_mapped(diagram: &Diagram) -> Result<(String, Map), Declined> {
    let skin = Skin::from_params(&diagram.frame.skinparams)?;
    let mut canvas = Canvas::new(&skin.font_name, skin.monochrome);
    canvas.sketch(skin.sketch);
    canvas.shadows(skin.shadow);
    let family = canvas.family();
    let notes = Annotations::new(
        &diagram.frame,
        [skin.title, skin.header, skin.footer, skin.caption],
        family,
    )?;
    let mut bands = Vec::new();
    if !diagram.activities.is_empty() || !diagram.releases.is_empty() {
        bands = paint(&mut canvas, diagram, &skin, family, notes.top())?;
    }
    notes.draw(&mut canvas);
    let map = canvas.place(
        Map {
            bands,
            ..Map::default()
        },
        MARGIN,
    );
    Ok((canvas.finish(MARGIN, skin.background), map))
}

/// A band across the map: a release, or the stories planned in none.
struct Slice {
    release: Option<usize>,
    name: Block,
    goal: Block,
}

/// A column under the backbone: a step of an activity, or the place of
/// an activity that has none yet.
struct Column<'a> {
    x: f32,
    step: Option<&'a Step>,
}

/// The stories of one band in one column, top down: each one's words as
/// written, which its hand is drawn by, and as measured.
type Stack<'a> = Vec<(&'a str, Block)>;

/// The fonts of the cards, each in a color that reads on its fill.
struct Fonts {
    activity: Font,
    step: Font,
    story: Font,
    name: Font,
    goal: Font,
}

impl Fonts {
    fn of(skin: &Skin) -> Fonts {
        let label = skin.label;
        let paper = skin.paper();
        Fonts {
            activity: Font {
                size: label.size + 1.0,
                color: skin.ink_on(skin.accent),
                ..label
            }
            .bold(),
            step: Font {
                color: skin.ink_on(skin.card),
                ..label
            }
            .bold(),
            story: Font {
                size: skin.story_size.unwrap_or(label.size),
                color: skin.story_color.unwrap_or_else(|| skin.ink_on(skin.note)),
                ..label
            },
            name: label.bold(),
            // Quieter than the name, but read on the page and on a tinted
            // band alike.
            goal: Font {
                size: (label.size - 1.0).max(8.0),
                color: paper
                    .mix(label.color, 0.65)
                    .legible_on(paper, label.color, 4.5)
                    .legible_on(skin.tint(), label.color, 4.5),
                ..label
            }
            .italic(),
        }
    }
}

/// Paints the map below `top`; the bands a pane keeps in sight.
fn paint(
    canvas: &mut Canvas,
    diagram: &Diagram,
    skin: &Skin,
    family: Family,
    top: f32,
) -> Result<Vec<Band>, Declined> {
    let fonts = Fonts::of(skin);
    let paper = skin.paper();
    let column = (skin.label.size * 12.0).max(150.0);
    let inner = column - 2.0 * PAD;
    let slices = slices(diagram, &fonts, family)?;
    let gutter = if diagram.releases.is_empty() {
        0.0
    } else {
        slices
            .iter()
            .map(|s| s.name.width.max(s.goal.width))
            .fold(0.0, f32::max)
            + 2.0 * PAD
    };
    // The columns, and where each activity spans them.
    let left = if gutter > 0.0 { gutter + GAP } else { 0.0 };
    let mut x = left;
    let mut columns: Vec<Column> = Vec::new();
    let mut spans: Vec<(f32, f32)> = Vec::new();
    for activity in &diagram.activities {
        let from = x;
        if activity.steps.is_empty() {
            columns.push(Column { x, step: None });
            x += column + GAP;
        }
        for step in &activity.steps {
            columns.push(Column {
                x,
                step: Some(step),
            });
            x += column + GAP;
        }
        spans.push((from, x - GAP));
        x += APART - GAP;
    }
    let right = match spans.last() {
        Some(&(_, end)) => end,
        None => left + column,
    };
    // The backbone: the activities, and the steps where one has a name.
    let mut names = Vec::new();
    for (activity, &(from, to)) in diagram.activities.iter().zip(&spans) {
        let words = wrap(
            &activity.name,
            fonts.activity,
            family,
            to - from - 2.0 * PAD,
            activity.line,
        )?;
        names.push(words.written_on(activity.line));
    }
    let activity_h = row_height(&names, fonts.activity);
    let mut steps = Vec::new();
    for c in &columns {
        steps.push(
            match c.step.and_then(|s| s.name.as_ref().map(|n| (n, s.line))) {
                Some((name, line)) => {
                    Some(wrap(name, fonts.step, family, inner, line)?.written_on(line))
                }
                None => None,
            },
        );
    }
    let named_steps: Vec<Block> = steps.iter().flatten().cloned().collect();
    let step_h = if named_steps.is_empty() {
        0.0
    } else {
        row_height(&named_steps, fonts.step)
    };
    let backbone = !diagram.activities.is_empty();
    let steps_top = top + activity_h + ROWS;
    let backbone_bottom = match (backbone, step_h > 0.0) {
        (false, _) => top,
        (true, false) => top + activity_h,
        (true, true) => steps_top + step_h,
    };
    // The stories of each band in each column, measured.
    let mut bands_top = if backbone {
        backbone_bottom + BELOW
    } else {
        top
    };
    let first_band = bands_top;
    let mut laid: Vec<(f32, f32, Vec<Stack>)> = Vec::new();
    for slice in &slices {
        let mut stacks = Vec::new();
        let mut fullest: f32 = 0.0;
        for c in &columns {
            let mut stack = Vec::new();
            let mut height: f32 = 0.0;
            for story in c.step.iter().flat_map(|s| &s.stories) {
                if story.release != slice.release {
                    continue;
                }
                let words = wrap(&story.text, fonts.story, family, inner, story.line)?;
                height += words.height + 2.0 * PAD + GAP;
                stack.push((story.text.as_str(), words.written_on(story.line)));
            }
            fullest = fullest.max(height - GAP);
            stacks.push(stack);
        }
        let label = slice.name.height
            + if slice.goal.is_empty() {
                0.0
            } else {
                2.0 + slice.goal.height
            };
        let height = (fullest.max(label) + 2.0 * BAND_PAD).max(44.0);
        laid.push((bands_top, height, stacks));
        bands_top += height;
    }
    let bottom = bands_top;
    // The bands: every other one tinted, a rule between them. The rules
    // only guide the eye, as a chart's grid does, and are drawn as faint.
    let tint = skin.tint();
    let rule = Stroke::new(paper.mix(skin.label.color, 0.3), 1.0).dashed(5.0, 4.0);
    canvas.begin_group(Layer::Background, crate::GRID);
    for (i, (y, h, _)) in laid.iter().enumerate() {
        if i % 2 == 1 {
            canvas.plain(|c| {
                c.rect(
                    Layer::Background,
                    (0.0, *y, right, *h),
                    0.0,
                    Some(tint),
                    None,
                )
            });
        }
        if slices.len() > 1 || !diagram.releases.is_empty() {
            canvas.line(Layer::Background, (0.0, *y), (right, *y), rule);
        }
    }
    if !diagram.releases.is_empty() {
        canvas.line(Layer::Background, (0.0, bottom), (right, bottom), rule);
    }
    canvas.end_group(Layer::Background);
    // Each activity, step, release and story named by its words
    // (docs/adr/0037), the map drawn as a whole.
    canvas.whole();
    let mut given = crate::svg::Given::default();
    let mut part = |kind: &str, words: &str| {
        given.unique(crate::svg::element(kind, crate::svg::first_line(words)))
    };
    // The backbone.
    for ((activity, &(from, to)), words) in diagram.activities.iter().zip(&spans).zip(&names) {
        let name = part("activity", &activity.name);
        canvas.part(&name, |canvas| {
            canvas.named(&format!("activity {}", activity.name), |c| {
                c.rect(
                    Layer::Bars,
                    (from, top, to - from, activity_h),
                    skin.round,
                    Some(skin.accent),
                    Some(Stroke::new(skin.accent, 1.0)),
                );
                c.text(
                    Layer::Front,
                    words,
                    ((from + to) / 2.0, top + (activity_h - words.height) / 2.0),
                    Align::Center,
                );
            });
        });
    }
    for (c, words) in columns.iter().zip(&steps) {
        let (Some(step), Some(words)) = (c.step, words) else {
            continue;
        };
        let name = part("step", step.name.as_deref().unwrap_or_default());
        canvas.part(&name, |canvas| {
            canvas.named(
                &format!("step {}", step.name.as_deref().unwrap_or_default()),
                |canvas| {
                    canvas.rect(
                        Layer::Bars,
                        (c.x, steps_top, column, step_h),
                        skin.round,
                        Some(skin.card),
                        Some(Stroke::new(skin.card_border, 1.0)),
                    );
                    canvas.text(
                        Layer::Front,
                        words,
                        (
                            c.x + column / 2.0,
                            steps_top + (step_h - words.height) / 2.0,
                        ),
                        Align::Center,
                    );
                },
            );
        });
    }
    // The releases' names, and the stories in their bands.
    for (slice, (y, _, stacks)) in slices.iter().zip(&laid) {
        if gutter > 0.0 {
            // The stories of no release stand in a band of their own.
            let said = slice
                .release
                .and_then(|at| diagram.releases.get(at))
                .map_or("unplanned", |release| release.name.as_str());
            let name = part("release", said);
            canvas.part(&name, |canvas| {
                canvas.text(Layer::Front, &slice.name, (PAD, y + BAND_PAD), Align::Left);
                canvas.text(
                    Layer::Front,
                    &slice.goal,
                    (PAD, y + BAND_PAD + slice.name.height + 2.0),
                    Align::Left,
                );
            });
        }
        for (c, stack) in columns.iter().zip(stacks) {
            let mut at = y + BAND_PAD;
            for (text, words) in stack {
                let h = words.height + 2.0 * PAD;
                // Seeded by its words, not its line: a line written above
                // it does not redraw its hand (docs/adr/0023).
                let name = part("story", text);
                canvas.part(&name, |canvas| {
                    canvas.named(&format!("story {text}"), |canvas| {
                        canvas.rect(
                            Layer::Bars,
                            (c.x, at, column, h),
                            skin.round.min(3.0),
                            Some(skin.note),
                            Some(Stroke::new(skin.note_border, 1.0)),
                        );
                        canvas.text(Layer::Front, words, (c.x + PAD, at + PAD), Align::Left);
                    });
                });
                at += h + GAP;
            }
        }
    }
    // What a pane keeps in sight: the backbone over the bands, the
    // releases' names beside them.
    let mut held = Vec::new();
    let over = Rect {
        x: left,
        y: first_band,
        w: right - left,
        h: bottom - first_band,
    };
    if backbone {
        held.push(Band {
            rect: Rect {
                x: left,
                y: top,
                w: right - left,
                h: backbone_bottom - top,
            },
            edge: Edge::Top,
            over,
        });
    }
    if gutter > 0.0 {
        held.push(Band {
            rect: Rect {
                x: 0.0,
                y: first_band,
                w: gutter,
                h: bottom - first_band,
            },
            edge: Edge::Left,
            over,
        });
    }
    Ok(held)
}

/// The bands: each release in its order, then the stories planned in
/// none, where there are releases and such stories; one band without a
/// name where there are no releases.
fn slices(diagram: &Diagram, fonts: &Fonts, family: Family) -> Result<Vec<Slice>, Declined> {
    let mut slices = Vec::new();
    for (index, release) in diagram.releases.iter().enumerate() {
        let name = wrap(&release.name, fonts.name, family, GUTTER, release.line)?
            .written_at(release.line, Some(release.col));
        let goal = match &release.goal {
            Some(goal) => {
                wrap(goal, fonts.goal, family, GUTTER, release.line)?.written_on(release.line)
            }
            None => Block::empty(),
        };
        slices.push(Slice {
            release: Some(index),
            name,
            goal,
        });
    }
    let unplanned = diagram.stories().any(|s| s.release.is_none());
    if slices.is_empty() || unplanned {
        let name = if slices.is_empty() {
            Block::empty()
        } else {
            wrap(UNPLANNED, fonts.goal, family, GUTTER, 0)?
        };
        slices.push(Slice {
            release: None,
            name,
            goal: Block::empty(),
        });
    }
    Ok(slices)
}

/// The height of a row of cards: its tallest words, padded; a line of
/// `font` at least.
fn row_height(words: &[Block], font: Font) -> f32 {
    let tallest = words
        .iter()
        .map(|w| w.height)
        .fold(font.size * 1.2, f32::max);
    tallest + 2.0 * PAD
}

/// `words` in `font`, broken into lines no wider than `width`.
fn wrap(
    words: &str,
    font: Font,
    family: Family,
    width: f32,
    line: usize,
) -> Result<Block, Declined> {
    text::wrapped(words, font, family, width.max(20.0)).map_err(|reason| Declined::at(line, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOP: &str = "@startstorymap\ntitle Shop\nrelease MVP : sell the first book\nrelease Later\nactivity Find a book\n  step Search\n    * By title : MVP\n    * By author : Later\n  step Browse\n    * The shelves\nactivity Buy\n  step Pay\n    * By card : MVP\n    * By invoice : Later\n@endstorymap\n";

    fn drawn(source: &str) -> (String, Map) {
        crate::render_mapped(source, None).unwrap()
    }

    #[test]
    fn the_backbone_stands_over_the_releases_and_each_story_in_its_band() {
        let (svg, map) = drawn(SHOP);
        for words in [
            "Find a book",
            "Search",
            "By title",
            "MVP",
            "sell the first book",
            "Later",
            UNPLANNED,
        ] {
            assert!(svg.contains(words), "{words}: {svg}");
        }
        let at = |line: usize| {
            map.tied
                .iter()
                .find(|t| t.line == line)
                .unwrap_or_else(|| panic!("line {line} is tied: {:?}", map.tied))
                .rect
        };
        // An activity over its steps, a step over its stories.
        assert!(at(4).y < at(5).y && at(5).y < at(6).y);
        // The activity spans its two steps.
        assert!(at(4).x > at(5).x && at(4).x < at(8).x);
        // A story in MVP above one in Later, in the same column; one
        // planned in none below both.
        assert!(at(6).y < at(7).y);
        assert!((at(6).x - at(7).x).abs() < 0.5);
        assert!(at(9).y > at(7).y);
        // The stories of one release stand side by side across steps.
        assert!((at(6).y - at(12).y).abs() < 0.5);
        // A release's name and its goal are both its line's.
        let release: Vec<_> = map.tied.iter().filter(|t| t.line == 2).collect();
        assert_eq!(release.len(), 2);
        assert!(release.iter().any(|t| t.col == Some("release ".len())));
        // The backbone is held at the top, the names at the left.
        assert_eq!(map.bands.len(), 2);
        assert!(map.bands.iter().any(|b| b.edge == Edge::Top));
        assert!(map.bands.iter().any(|b| b.edge == Edge::Left));
    }

    #[test]
    fn a_map_without_releases_has_one_band_and_no_names() {
        let (svg, map) = drawn("@startstorymap\nactivity Buy\n* Pay\n* Ship\n@endstorymap\n");
        assert!(svg.contains("Pay") && !svg.contains(UNPLANNED), "{svg}");
        assert_eq!(map.bands.len(), 1);
        // An activity with no step has its stories right under it.
        let empty = drawn("@startstorymap\n@endstorymap\n").0;
        assert!(empty.starts_with("<svg"), "{empty}");
        let releases = drawn("@startstorymap\nrelease MVP\n@endstorymap\n").0;
        assert!(releases.contains("MVP"), "{releases}");
    }

    #[test]
    fn a_theme_colors_the_cards_and_their_words_read() {
        let themed = SHOP.replace(
            "title Shop\n",
            "title Shop\nskinparam backgroundColor #16181d\nskinparam defaultFontColor #e3e5ea\nskinparam RectangleBackgroundColor #23272f\nskinparam RectangleBorderColor #79a6f0\nskinparam NoteBackgroundColor #3a3526\n",
        );
        let svg = drawn(&themed).0.to_ascii_uppercase();
        assert!(svg.contains("#79A6F0") && svg.contains("#3A3526"), "{svg}");
        // Long words wrap inside their card.
        let long = SHOP.replace(
            "By title",
            "By the title, the subtitle or the words on its back cover",
        );
        let (_, map) = drawn(&long);
        let story = map.tied.iter().find(|t| t.line == 6).unwrap().rect;
        let step = map.tied.iter().find(|t| t.line == 5).unwrap().rect;
        assert!(story.h > step.h * 1.5);
    }

    #[test]
    fn drawn_by_hand_it_keeps_its_cards() {
        let sketched = SHOP.replace(
            "title Shop\n",
            "title Shop\nskinparam handwritten true\nskinparam shadowing true\n",
        );
        let svg = drawn(&sketched).0;
        assert!(svg.contains("By card"), "{svg}");
    }
}
