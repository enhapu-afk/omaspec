//! A fold as a page: the containers of a diagram folding to their boxes
//! and unfolding again, in vectors, sharp at any size
//! ([ADR 0031](../../../../docs/adr/0031-a-presentation-as-one-page.md)).
//!
//! The page holds the picture before the first fold and the one after
//! each, drawn with their parts marked
//! ([ADR 0029](../../../../docs/adr/0029-a-glide-by-parts.md)), and the
//! glide from each to the next as the pane works it out: which box goes
//! from where to where, which goes or comes, each line's route before and
//! after. The script only walks them: it moves each part's own group,
//! and draws each line between its two routes.

use omaspec_uml::Color;
use omaspec_uml::map::{Head, HeadFill};
use serde_json::{Value, json};

use super::{ExportError, Source};

/// The pictures of a fold and the glides between them.
#[derive(Debug, Clone)]
pub struct Folding {
    /// What folds, in order: the picture `n` has the first `n` folded.
    pub names: Vec<String>,
    /// What each picture is, where the pictures are a diagram's history
    /// rather than its folds: its commit, said under it. A history plays
    /// on from its first picture to its last and starts again.
    pub captions: Vec<String>,
    /// One picture more than names, each with its parts marked.
    pub pictures: Vec<Picture>,
    /// The glide from each picture to the next.
    pub glides: Vec<Glide>,
    /// The page's ground, and the color of what it says.
    pub paper: Color,
    pub accent: Color,
}

/// A picture as the engine drew it, and its size in its own units.
#[derive(Debug, Clone)]
pub struct Picture {
    pub svg: String,
    pub size: (f32, f32),
}

/// A rectangle: left, top, width, height.
pub type Rect = [f32; 4];

/// Whether a box is in both pictures, only in the new one, or only in
/// the one before.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Stay,
    Arrive,
    Leave,
}

/// What happened to a box from one version of a diagram to the next, as
/// it glows a moment in a history: it came, it changed (its size), or it
/// went. The colors are those `omaspec diff -o` marks a machine in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Came,
    Changed,
    Went,
}

/// The glide from one picture to the next.
#[derive(Debug, Clone, Default)]
pub struct Glide {
    /// The boxes, the ones around others first.
    pub boxes: Vec<Moved>,
    pub lines: Vec<Line>,
}

/// One box on its way.
#[derive(Debug, Clone)]
pub struct Moved {
    /// The part it is drawn in, before and after.
    pub parts: [String; 2],
    pub from: Rect,
    pub to: Rect,
    pub role: Role,
    /// The box of a tree it goes into or comes out of: where that one
    /// was and where it is.
    pub into: Option<(Rect, Rect)>,
    /// How it glows, in a history: none in a fold, or where it only moved.
    pub glow: Option<Change>,
}

/// One line on its way: its route before (none for one that comes) and
/// after (none for one that goes).
#[derive(Debug, Clone)]
pub struct Line {
    pub from: Option<Route>,
    pub to: Option<Route>,
    pub colors: [Color; 2],
    pub width: f32,
    pub dash: Option<(f32, f32)>,
    /// A link of a tree that goes into the box above with its box, or
    /// comes out of it.
    pub fold: Option<Role>,
    /// The parts its words are drawn in, before and after: in its
    /// middle, beside its start, beside its end.
    pub words: [[Option<String>; 3]; 2],
}

/// A line's route, as many points before as after, and the marks at its
/// ends.
#[derive(Debug, Clone)]
pub struct Route {
    pub points: Vec<(f32, f32)>,
    pub heads: Vec<Head>,
}

/// The page that plays `folding`, called `title`, the diagram's `source`
/// in its first picture. Each picture's ids are made its own and it is
/// sanitized; the first carries the faces the words of all of them are
/// set in.
pub fn page(folding: &Folding, source: Option<Source>, title: &str) -> Result<String, ExportError> {
    let mut pictures = String::new();
    for (at, picture) in folding.pictures.iter().enumerate() {
        let svg = if at == 0 {
            let others: Vec<&str> = folding.pictures[1..]
                .iter()
                .map(|picture| picture.svg.as_str())
                .collect();
            crate::fonts::embed_with(&picture.svg, &others)
        } else {
            picture.svg.clone()
        };
        // Only the source is written after the picture is sanitized.
        let svg = crate::atlas::own_ids(&svg, &prefix(at));
        let svg = crate::sanitize::sanitize(&svg).map_err(|e| ExportError::Svg(e.to_string()))?;
        let svg = match (at, source) {
            (0, Some(s)) => omaspec_core::embed::svg_with_source(&svg, s.code, s.language, s.file),
            _ => svg,
        };
        pictures.push_str(&svg);
        pictures.push('\n');
    }
    Ok(super::present::written(
        &pictures,
        &data(folding),
        (folding.paper, folding.accent),
        title,
    ))
}

/// What the ids of picture `at` start with: two pictures in one page
/// name the same parts.
fn prefix(at: usize) -> String {
    format!("p{at}-")
}

/// The class of the groups of `part`, which the page finds within the
/// picture `at` of the glide's two (docs/adr/0037).
fn part(_at: usize, part: &str) -> String {
    omaspec_uml::part_id(part)
}

fn data(folding: &Folding) -> Value {
    let sizes: Vec<Value> = folding
        .pictures
        .iter()
        .map(|p| json!([round(p.size.0), round(p.size.1)]))
        .collect();
    let glides: Vec<Value> = folding
        .glides
        .iter()
        .enumerate()
        .map(|(at, glide)| {
            let boxes: Vec<Value> = glide
                .boxes
                .iter()
                .map(|b| {
                    json!({
                        "parts": [part(at, &b.parts[0]), part(at + 1, &b.parts[1])],
                        "from": rect(b.from),
                        "to": rect(b.to),
                        "role": role(b.role),
                        "into": b.into.map(|(was, is)| json!([rect(was), rect(is)])),
                        "glow": b.glow.map(|change| match change {
                            Change::Came => "came",
                            Change::Changed => "changed",
                            Change::Went => "went",
                        }),
                    })
                })
                .collect();
            let lines: Vec<Value> = glide
                .lines
                .iter()
                .map(|line| {
                    let words = |side: usize| -> Vec<Value> {
                        line.words[side]
                            .iter()
                            .map(|name| match name {
                                Some(name) => json!(part(at + side, name)),
                                None => Value::Null,
                            })
                            .collect()
                    };
                    json!({
                        "from": line.from.as_ref().map(route),
                        "to": line.to.as_ref().map(route),
                        "colors": [line.colors[0].to_string(), line.colors[1].to_string()],
                        "width": round(line.width),
                        "dash": line.dash.map(|(on, off)| json!([round(on), round(off)])),
                        "fold": line.fold.map(role),
                        "words": [words(0), words(1)],
                    })
                })
                .collect();
            json!({ "boxes": boxes, "lines": lines })
        })
        .collect();
    json!({
        "kind": if folding.captions.is_empty() { "fold" } else { "history" },
        "names": folding.names,
        "captions": folding.captions,
        "sizes": sizes,
        "lines": (0..folding.pictures.len())
            .map(|at| format!("{}{}", prefix(at), omaspec_uml::LINES))
            .collect::<Vec<_>>(),
        "glides": glides,
        // The colors what came, changed and went glows in, where it does.
        "glows": folding
            .glides
            .iter()
            .any(|glide| glide.boxes.iter().any(|b| b.glow.is_some()))
            .then(|| {
                json!({
                    "came": omaspec_uml::state::TAKEN,
                    "changed": omaspec_uml::state::CHANGED,
                    "went": omaspec_uml::state::MISSED,
                })
            }),
    })
}

fn route(route: &Route) -> Value {
    let heads: Vec<Value> = route
        .heads
        .iter()
        .map(|head| {
            json!({
                "points": points(&head.points),
                "closed": head.closed,
                "fill": match head.fill {
                    HeadFill::Ink => "ink",
                    HeadFill::Paper => "paper",
                    HeadFill::None => "none",
                },
            })
        })
        .collect();
    json!({ "points": points(&route.points), "heads": heads })
}

fn points(points: &[(f32, f32)]) -> Vec<Value> {
    points
        .iter()
        .map(|p| json!([round(p.0), round(p.1)]))
        .collect()
}

fn rect(r: Rect) -> Value {
    json!([round(r[0]), round(r[1]), round(r[2]), round(r[3])])
}

fn role(role: Role) -> &'static str {
    match role {
        Role::Stay => "stay",
        Role::Arrive => "arrive",
        Role::Leave => "leave",
    }
}

/// `value` to a hundredth, without the float's noise in the page.
fn round(value: f32) -> f64 {
    (f64::from(value) * 100.0).round() / 100.0
}
