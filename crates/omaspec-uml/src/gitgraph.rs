//! Mermaid git graphs.
//!
//! The source is read here, statement by statement, as git would run it:
//! `commit`, `branch`, `checkout` (or `switch`), `merge` and `cherry-pick`.
//! Each branch is a lane, each commit a mark on its lane at the place its
//! turn gives it. `gitGraph LR` (the default) lets time run to the right
//! with the lanes below one another; `TB` lets it run down and `BT` up,
//! the lanes beside one another.
//!
//! A commit shows the id its writer gave it and its tag. An id Mermaid
//! would make up (`3-a1b2c3d`) is not shown: it says nothing and differs
//! with every drawing. What git itself would refuse (a branch that is not
//! there, a merge of a branch into itself) is refused here by name, and
//! the diagram is left to the engine that reports it.

use crate::Declined;
use crate::color::{Color, Paint};
use crate::common::Text;
use crate::svg::{Align, Canvas, Layer, Stroke};
use crate::text::{self, Block, Font};

mod read;
mod settings;

pub use settings::{SETTINGS, Settings};

/// The room one turn takes along the way time runs.
const STEP: f32 = 56.0;
/// The room a lane takes when nothing written beside it asks for more.
const LANE: f32 = 64.0;
/// A commit's mark.
const RADIUS: f32 = 9.0;
/// How round a line turns from one lane to another.
const TURN: f32 = 14.0;
/// Between a branch's label and its first turn.
const LEAD: f32 = 28.0;
/// The default lanes: Mermaid's `git0` to `git7`. A theme replaces them.
const LANES: [u32; 8] = [
    0x6E6ECC, 0xC9C95A, 0x8FB85A, 0xA06ECC, 0xCC6ECC, 0xCC6E9D, 0xCC6E6E, 0xCC9D6E,
];

/// A git graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Diagram {
    pub title: Option<Text>,
    pub direction: Direction,
    /// The branches in the order of their lanes.
    pub branches: Vec<Branch>,
    /// The commits in the order they were made.
    pub commits: Vec<Commit>,
    pub look: Look,
    pub settings: Settings,
}

/// The way time runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// To the right, the lanes below one another.
    Across,
    /// Down the page, the lanes beside one another.
    Down,
    /// Up the page.
    Up,
}

/// A branch and its lane.
#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    pub name: String,
    pub line: usize,
}

/// One commit.
#[derive(Debug, Clone, PartialEq)]
pub struct Commit {
    /// The id its writer gave it; none for one Mermaid would make up.
    pub id: Option<String>,
    pub tag: Option<String>,
    pub kind: Kind,
    /// Its branch, by its place in [`Diagram::branches`].
    pub branch: usize,
    /// The commit before it on its branch, or the one its branch started
    /// from; none for the first commit of all.
    pub parent: Option<usize>,
    /// What a merge took in, or what a cherry-pick copied.
    pub from: Option<From>,
    pub line: usize,
}

/// The second commit a commit comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum From {
    /// The head of the branch a merge took in.
    Merged(usize),
    /// The commit a cherry-pick copied.
    Picked(usize),
}

/// How a commit is marked: `type: NORMAL`, `HIGHLIGHT` or `REVERSE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Normal,
    Highlight,
    Reverse,
}

/// Type, ink, page and the lanes' colors. The default colors are
/// Mermaid's; a theme replaces them.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub font_name: String,
    /// Mermaid's `look: handDrawn`: shapes drawn by hand (docs/adr/0023).
    pub sketch: bool,
    pub label_size: f32,
    pub title_size: f32,
    pub ink: Color,
    pub background: Paint,
    pub lanes: [Color; 8],
}

impl Default for Look {
    fn default() -> Self {
        Self {
            font_name: crate::FACE.into(),
            sketch: false,
            label_size: 14.0,
            title_size: 20.0,
            ink: Color::hex(0x333333),
            background: None,
            lanes: LANES.map(Color::hex),
        }
    }
}

/// The git graph `source` describes, or `None` when it is not a
/// `gitGraph` or it uses a construct this reader refuses.
pub fn from_mermaid(source: &str) -> Option<Diagram> {
    from_mermaid_with(source, &Settings::default())
}

/// The git graph `source` describes with its `settings`.
pub fn from_mermaid_with(source: &str, settings: &Settings) -> Option<Diagram> {
    read::read(source, settings).ok()
}

/// The first construct [`from_mermaid_with`] refuses by name, and its
/// line. A source that is not a git graph, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str, settings: &Settings) -> Option<(usize, &'static str)> {
    match read::read(source, settings) {
        Err(read::Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
}

/// What Mermaid refuses in a git graph as well, and its line: a second
/// `gitGraph` in the same text. None is drawn then, by mmdr either.
pub fn mermaid_wrong(source: &str) -> Option<(usize, String)> {
    match read::read(source, &Settings::default()) {
        Err(read::Stop::Wrong(reason)) => Some((
            reason.line,
            format!(
                "{} in the same text: a Mermaid text holds one diagram, and Mermaid refuses this one too",
                reason.what
            ),
        )),
        _ => None,
    }
}

/// The plate a branch's name stands on, as wide and high as it is: room
/// round the name, and never narrower than a short word asks, so a name of
/// two narrow letters (`jj`) is not squeezed into one (REQ-100).
fn plate(name: &Block, look: &Look) -> (f32, f32) {
    (
        (name.width + 24.0).max(look.label_size * 2.8),
        name.height + 10.0,
    )
}

/// The sine of 45°: how far turned words reach across and down.
const HALF: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// The words of each commit on `lane`.
fn lane_words<'a>(
    diagram: &'a Diagram,
    words: &'a [Words],
    lane: usize,
) -> impl Iterator<Item = &'a Words> {
    diagram
        .commits
        .iter()
        .zip(words)
        .filter(move |(commit, _)| commit.branch == lane)
        .map(|(_, words)| words)
}

/// How far past the room beside a lane's marks its turned ids start, as
/// time runs down or up: past the widest of its tags.
fn id_column(diagram: &Diagram, words: &[Words], lane: usize) -> f32 {
    lane_words(diagram, words, lane)
        .filter_map(|w| w.tag.as_ref())
        .map(|tag| tag.width + 20.0)
        .fold(0.0, f32::max)
}

/// `id` turned by 45°, its right end at `anchor` and its words running down
/// to the left, on a plate of the page's color.
fn turned(canvas: &mut Canvas, id: &Block, anchor: (f32, f32), paper: Color) {
    plated(canvas, id, anchor, Align::Right, paper);
}

/// `id` turned by 45°, its left end at `anchor` and its words rising to
/// the right, on a plate of the page's color.
fn rising(canvas: &mut Canvas, id: &Block, anchor: (f32, f32), paper: Color) {
    plated(canvas, id, anchor, Align::Left, paper);
}

/// `id` turned by 45° about `anchor`, which `align` puts on its top line,
/// on a plate of the page's color.
fn plated(canvas: &mut Canvas, id: &Block, anchor: (f32, f32), align: Align, paper: Color) {
    let (x, y) = anchor;
    // The plate's corners, turned as the words are: along them up to the
    // right, and across them down to the right.
    let along = |d: f32| (d * HALF, -d * HALF);
    let across = |d: f32| (d * HALF, d * HALF);
    let corner = |a: f32, b: f32| {
        let (ax, ay) = along(a);
        let (bx, by) = across(b);
        (x + ax + bx, y + ay + by)
    };
    // Along the words from where they start to where they end, 3 past
    // them either way.
    let (from, to) = match align {
        Align::Right => (-id.width - 3.0, 3.0),
        Align::Left => (-3.0, id.width + 3.0),
        Align::Center => (-id.width / 2.0 - 3.0, id.width / 2.0 + 3.0),
    };
    let h = id.height + 1.0;
    canvas.polygon(
        Layer::Bars,
        &[
            corner(from, -1.0),
            corner(to, -1.0),
            corner(to, h),
            corner(from, h),
        ],
        Some(paper),
        None,
    );
    canvas.text_turned(Layer::Front, id, anchor, align, -45.0);
}

/// How far a lane begins before its first turn: [`LEAD`], or more where
/// the first turn's words reach `reach` back from it, so that they keep 4
/// units from the plates, which end 8 before the lane begins.
fn lead_for(reach: f32) -> f32 {
    LEAD.max(reach + 4.0 - 8.0)
}

/// Where a lane's plate stands, its lane beginning at `from`: before it
/// as time runs across, over it as time runs down, under it as up.
fn plate_at(plan: &Plan, name: &Block, look: &Look, from: (f32, f32)) -> (f32, f32, f32, f32) {
    let (w, h) = plate(name, look);
    let (x, y) = match plan.direction {
        Direction::Across => (from.0 - 8.0 - w, from.1 - h / 2.0),
        Direction::Down => (from.0 - w / 2.0, from.1 - 8.0 - h),
        Direction::Up => (from.0 - w / 2.0, from.1 + 8.0),
    };
    (x, y, w, h)
}

/// What stands written beside a commit, measured.
struct Words {
    id: Option<Block>,
    tag: Option<Block>,
}

impl Words {
    /// How far the words reach from the mark, the gap between them counted.
    fn reach(&self) -> f32 {
        let id = self.id.as_ref().map_or(0.0, |block| block.width + 8.0);
        let tag = self.tag.as_ref().map_or(0.0, |block| block.width + 20.0);
        id + tag
    }
}

/// Where everything lies: along the way time runs (`t`) and across the
/// lanes (`l`), turned into the page's x and y by the direction.
struct Plan {
    direction: Direction,
    /// How far a lane begins before its first turn ([`lead_for`]).
    lead: f32,
    /// Each lane's middle, across.
    lanes: Vec<f32>,
    /// Where the first turn lies, and how far one is from the next.
    start: f32,
    step: f32,
    /// The last turn's place, for a time that runs up.
    end: f32,
    top: f32,
}

impl Plan {
    fn at(&self, turn: usize, lane: usize) -> (f32, f32) {
        let t = self.start + turn as f32 * self.step;
        self.page(t, self.lanes[lane])
    }

    fn page(&self, t: f32, l: f32) -> (f32, f32) {
        match self.direction {
            Direction::Across => (t, self.top + l),
            Direction::Down => (l, self.top + t),
            Direction::Up => (l, self.top + self.end + self.start - t),
        }
    }
}

/// The SVG. Words that the text engine cannot draw decline the graph.
pub fn draw(diagram: &Diagram) -> Result<String, Declined> {
    draw_mapped(diagram).map(|(svg, _)| svg)
}

/// The SVG, and where the words of each commit's line stand: its id and
/// its tag, which that line alone writes.
pub fn draw_mapped(diagram: &Diagram) -> Result<(String, crate::map::Map), Declined> {
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
    let top = title.as_ref().map_or(0.0, |block| block.height + 16.0);

    let names = diagram
        .branches
        .iter()
        .enumerate()
        .map(|(lane, branch)| {
            let fill = lane_color(look, lane);
            text::block(
                &branch.name,
                Font::new(look.label_size, on_fill(fill)),
                family,
            )
            .map_err(|reason| Declined::at(branch.line, reason))
        })
        .collect::<Result<Vec<Block>, Declined>>()?;
    let small = look.label_size * 0.82;
    let words = diagram
        .commits
        .iter()
        .map(|commit| {
            let block = |words: &Option<String>| match words {
                Some(words) => text::block(words, Font::new(small, look.ink), family)
                    .map(|block| Some(block.written_on(commit.line)))
                    .map_err(|reason| Declined::at(commit.line, reason)),
                None => Ok(None),
            };
            let id = commit
                .id
                .as_ref()
                .filter(|_| diagram.settings.show_commit_label)
                .cloned();
            Ok(Words {
                id: block(&id)?,
                tag: block(&commit.tag)?,
            })
        })
        .collect::<Result<Vec<Words>, Declined>>()?;

    let plan = plan(diagram, &names, &words, top);
    paint(&mut canvas, diagram, &plan, &names, &words);
    if let Some(block) = &title {
        let (left, right) = canvas.width_span();
        canvas.text(
            Layer::Front,
            block,
            ((left + right) / 2.0, 0.0),
            Align::Center,
        );
    }
    let map = canvas.place(crate::map::Map::default(), 10.0);
    Ok((canvas.finish(10.0, look.background), map))
}

/// Lays the lanes and the turns out so that no words meet.
fn plan(diagram: &Diagram, names: &[Block], words: &[Words], top: f32) -> Plan {
    let across = diagram.direction == Direction::Across;
    let turns = diagram.commits.len().max(1);
    let shown = diagram.settings.show_branches;
    let plates = names
        .iter()
        .filter(|_| shown)
        .map(|name| plate(name, &diagram.look));
    let (widest_plate, highest_plate) =
        plates.fold((0.0f32, 0.0f32), |(w, h), (pw, ph)| (w.max(pw), h.max(ph)));
    if across {
        // The names stand before the lanes; an id stands under its
        // commit, so the widest of them sets how far the turns are apart.
        // An id turned by 45° asks for no room along the lane, and its
        // lane stands as far from the next as it drops.
        let turned = diagram.settings.rotate_commit_label;
        let widest = words
            .iter()
            .flat_map(|w| [w.id.as_ref().filter(|_| !turned), w.tag.as_ref()])
            .flatten()
            .map(|block| block.width)
            .fold(0.0, f32::max);
        let step = STEP.max(widest + 14.0);
        let drop = words
            .iter()
            .filter(|_| turned)
            .filter_map(|w| w.id.as_ref())
            .map(|id| RADIUS + 4.0 + (id.width + id.height) * HALF)
            .fold(0.0, f32::max);
        let rise = words
            .iter()
            .filter_map(|w| w.tag.as_ref())
            .map(|tag| RADIUS + 8.0 + tag.height)
            .fold(RADIUS.max(highest_plate / 2.0), f32::max);
        // Lanes as far apart as their plates are high, at a large label
        // too, and as their turned ids drop.
        let lane =
            (highest_plate + 16.0)
                .max(LANE)
                .max(if drop > 0.0 { drop + rise + 6.0 } else { 0.0 });
        let lanes = (0..diagram.branches.len())
            .map(|at| at as f32 * lane + lane / 2.0)
            .collect();
        // The first turn's words, centred on it, keep clear of the plates
        // before it; turned, an id drops away from them.
        let reach = words.first().map_or(0.0, |first| {
            let id = first
                .id
                .as_ref()
                .filter(|_| !turned)
                .map_or(0.0, |id| id.width / 2.0 + 3.0);
            let tag = first.tag.as_ref().map_or(0.0, |tag| tag.width / 2.0 + 6.0);
            id.max(tag)
        });
        let lead = lead_for(reach);
        let start = widest_plate + 4.0 + lead;
        return Plan {
            direction: diagram.direction,
            lead,
            lanes,
            start,
            step,
            end: start + (turns - 1) as f32 * step,
            top,
        };
    }
    // The names stand over the lanes; the words stand beside their commit,
    // so each lane is as wide as its name and its longest words ask.
    // Turned, a lane's ids stand in a column after its tags, so they run
    // side by side and never meet.
    let turned = diagram.settings.rotate_commit_label;
    let mut lanes = Vec::with_capacity(names.len());
    let mut edge = 0.0;
    for (lane, name) in names.iter().enumerate() {
        let beside = if turned {
            let ids = lane_words(diagram, words, lane)
                .filter_map(|w| w.id.as_ref())
                .map(|id| (id.width + id.height) * HALF + 6.0)
                .fold(0.0, f32::max);
            id_column(diagram, words, lane) + ids
        } else {
            lane_words(diagram, words, lane)
                .map(Words::reach)
                .fold(0.0, f32::max)
        };
        let wide = if shown {
            plate(name, &diagram.look).0
        } else {
            0.0
        };
        let half = (wide / 2.0 + 2.0).max(RADIUS + 6.0);
        lanes.push(edge + half);
        edge += (half + (half.max(RADIUS + 6.0 + beside)) + 12.0).max(LANE);
    }
    // The first turn's words, beside it and as high as they are, keep
    // clear of the plates over (or under) it; a turned id rises as far as
    // it is long.
    let reach = words.first().map_or(0.0, |first| {
        let id = first.id.as_ref().map_or(0.0, |id| {
            if turned {
                id.width * HALF + id.height * HALF / 2.0 + 1.0
            } else {
                id.height / 2.0 + 1.0
            }
        });
        let tag = first.tag.as_ref().map_or(0.0, |tag| tag.height / 2.0 + 3.0);
        id.max(tag)
    });
    let start = highest_plate + 8.0 + lead_for(reach);
    Plan {
        direction: diagram.direction,
        lead: lead_for(reach),
        lanes,
        start,
        step: STEP,
        end: start + (turns - 1) as f32 * STEP,
        top,
    }
}

fn paint(canvas: &mut Canvas, diagram: &Diagram, plan: &Plan, names: &[Block], words: &[Words]) {
    let look = &diagram.look;
    let paper = look.background.unwrap_or(Color::WHITE);
    let across = plan.direction == Direction::Across;

    // Each lane: a faint line under its commits, and its name before it;
    // neither where the branches are not shown.
    let shown = if diagram.settings.show_branches {
        names
    } else {
        &[]
    };
    for (lane, name) in shown.iter().enumerate() {
        let color = lane_color(look, lane);
        let l = plan.lanes[lane];
        let from = plan.page(plan.start - plan.lead, l);
        let to = plan.page(plan.end + plan.step / 2.0, l);
        let faint = Stroke::new(color.mix(paper, 0.55), 1.5).dashed(3.0, 4.0);
        canvas.line(Layer::Background, from, to, faint);
        let (x, y, w, h) = plate_at(plan, name, look, from);
        canvas.card(Layer::Bars, (x, y, w, h), 4.0, color);
        canvas.text(
            Layer::Front,
            name,
            (x + w / 2.0, y + (h - name.height) / 2.0),
            Align::Center,
        );
    }

    // The lines between the commits, under their marks.
    for (turn, commit) in diagram.commits.iter().enumerate() {
        let here = plan.at(turn, commit.branch);
        let own = Stroke::new(lane_color(look, commit.branch), 3.0);
        if let Some(parent) = commit.parent {
            let before = &diagram.commits[parent];
            let there = plan.at(parent, before.branch);
            if before.branch == commit.branch {
                canvas.line(Layer::Frames, there, here, own);
            } else {
                // A branch leaves its commit across the lanes first, then
                // runs along its own.
                let corner = plan.at(parent, commit.branch);
                canvas.curve(Layer::Frames, &[there, corner, here], TURN, own);
            }
        }
        match commit.from {
            Some(From::Merged(head)) => {
                // What is merged runs along its own lane, then across.
                let merged = &diagram.commits[head];
                let there = plan.at(head, merged.branch);
                let corner = plan.at(turn, merged.branch);
                let stroke = Stroke::new(lane_color(look, merged.branch), 3.0);
                canvas.curve(Layer::Frames, &[there, corner, here], TURN, stroke);
            }
            Some(From::Picked(source)) => {
                // Straight across: along a lane it would lie under that
                // lane's own line.
                let picked = &diagram.commits[source];
                let there = plan.at(source, picked.branch);
                let stroke = Stroke::new(look.ink.mix(paper, 0.35), 1.5).dashed(4.0, 4.0);
                canvas.line(Layer::Frames, there, here, stroke);
            }
            None => {}
        }
    }

    for (turn, commit) in diagram.commits.iter().enumerate() {
        let at = plan.at(turn, commit.branch);
        let color = lane_color(look, commit.branch);
        mark(canvas, commit, at, color, paper);
        let written = &words[turn];
        if across {
            if let Some(id) = written
                .id
                .as_ref()
                .filter(|_| diagram.settings.rotate_commit_label)
            {
                // Turned, ending under the mark, on the page's color.
                turned(canvas, id, (at.0, at.1 + RADIUS + 4.0), paper);
            } else if let Some(id) = &written.id {
                // On the page's color: a branch that leaves here runs
                // down through the place of the words.
                let y = at.1 + RADIUS + 5.0;
                let plate = (
                    at.0 - id.width / 2.0 - 3.0,
                    y - 1.0,
                    id.width + 6.0,
                    id.height + 2.0,
                );
                canvas.rect(Layer::Bars, plate, 2.0, Some(paper), None);
                canvas.text(Layer::Front, id, (at.0, y), Align::Center);
            }
            if let Some(tag) = &written.tag {
                let y = at.1 - RADIUS - 8.0 - tag.height;
                label(canvas, tag, (at.0 - tag.width / 2.0, y), look, paper);
            }
        } else {
            let mut x = at.0 + RADIUS + 8.0;
            if diagram.settings.rotate_commit_label {
                // The tag level beside the mark, the id turned in its
                // lane's column after the tags, rising to the right.
                if let Some(tag) = &written.tag {
                    label(canvas, tag, (x + 4.0, at.1 - tag.height / 2.0), look, paper);
                }
                if let Some(id) = &written.id {
                    let column = x + id_column(diagram, words, commit.branch);
                    rising(canvas, id, (column, at.1 - id.height * HALF / 2.0), paper);
                }
                continue;
            }
            if let Some(id) = &written.id {
                // On the page's color: a line to a lane beside this one
                // passes here, and would strike the words through.
                let y = at.1 - id.height / 2.0;
                let plate = (x - 3.0, y - 1.0, id.width + 6.0, id.height + 2.0);
                canvas.rect(Layer::Bars, plate, 2.0, Some(paper), None);
                canvas.text(Layer::Front, id, (x, y), Align::Left);
                x += id.width + 10.0;
            }
            if let Some(tag) = &written.tag {
                label(canvas, tag, (x + 4.0, at.1 - tag.height / 2.0), look, paper);
            }
        }
    }
}

/// A commit's mark: a dot, a square for `HIGHLIGHT`, a crossed dot for
/// `REVERSE`, a ring for a merge, a dot with two seeds for a cherry-pick.
fn mark(canvas: &mut Canvas, commit: &Commit, (x, y): (f32, f32), color: Color, paper: Color) {
    let ring = Stroke::new(color, 3.0);
    match (commit.from, commit.kind) {
        (Some(From::Merged(_)), _) => {
            canvas.ellipse(
                Layer::Bars,
                (x, y),
                (RADIUS, RADIUS),
                Some(paper),
                Some(ring),
            );
            canvas.ellipse(Layer::Bars, (x, y), (3.5, 3.5), Some(color), None);
        }
        (Some(From::Picked(_)), _) => {
            canvas.ellipse(Layer::Bars, (x, y), (RADIUS, RADIUS), Some(color), None);
            for side in [-3.5, 3.5] {
                canvas.ellipse(
                    Layer::Bars,
                    (x + side, y + 1.5),
                    (2.2, 2.2),
                    Some(paper),
                    None,
                );
            }
        }
        (None, Kind::Highlight) => {
            let side = RADIUS * 2.0;
            canvas.rect(
                Layer::Bars,
                (x - RADIUS, y - RADIUS, side, side),
                2.0,
                Some(color),
                None,
            );
        }
        (None, Kind::Reverse) => {
            canvas.ellipse(Layer::Bars, (x, y), (RADIUS, RADIUS), Some(color), None);
            let arm = RADIUS * 0.45;
            let cross = Stroke::new(paper, 2.0);
            canvas.line(Layer::Bars, (x - arm, y - arm), (x + arm, y + arm), cross);
            canvas.line(Layer::Bars, (x - arm, y + arm), (x + arm, y - arm), cross);
        }
        (None, Kind::Normal) => {
            canvas.ellipse(Layer::Bars, (x, y), (RADIUS, RADIUS), Some(color), None);
        }
    }
}

/// A tag: its words on a plate with a border, the corner at `(x, y)`.
fn label(canvas: &mut Canvas, words: &Block, (x, y): (f32, f32), look: &Look, paper: Color) {
    let plate = look.ink.mix(paper, 0.88);
    canvas.rect(
        Layer::Bars,
        (x - 6.0, y - 3.0, words.width + 12.0, words.height + 6.0),
        3.0,
        Some(plate),
        Some(Stroke::new(look.ink.mix(paper, 0.5), 1.0)),
    );
    canvas.text(Layer::Front, words, (x, y), Align::Left);
}

fn lane_color(look: &Look, lane: usize) -> Color {
    look.lanes[lane % look.lanes.len()]
}

/// Black on a light fill, white on a dark one.
fn on_fill(fill: Color) -> Color {
    if fill.is_dark() {
        Color::WHITE
    } else {
        Color::BLACK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOW: &str = "gitGraph\n  commit id: \"start\"\n  branch develop\n  commit\n  commit id: \"feature\" tag: \"v0.9\"\n  checkout main\n  commit type: HIGHLIGHT\n  merge develop tag: \"v1.0\"\n  branch hotfix\n  commit type: REVERSE\n  checkout develop\n  cherry-pick id: \"start\"\n";

    /// A branch's name stands on a plate with room round it, and a short
    /// name of narrow letters (`jj`) is not squeezed into a plate that
    /// makes it read as one letter (REQ-100); the plate keeps clear of
    /// its lane's first commit and of the other lanes' plates, whichever
    /// way time runs, at a large label too.
    #[test]
    fn a_short_branch_name_has_room_on_its_plate() {
        for size in [14.0, 40.0] {
            let look = Look {
                label_size: size,
                ..Look::default()
            };
            let block = |name: &str| {
                text::block(name, Font::new(size, Color::BLACK), text::Family::Inter).unwrap()
            };
            let (short, long) = (block("jj"), block("feature/search"));
            let (w, h) = plate(&short, &look);
            assert!(w >= size * 2.8 && w >= short.width + 24.0, "{w}");
            assert!(h >= short.height + 10.0);
            assert_eq!(plate(&long, &look).0, long.width + 24.0);
            for head in ["gitGraph LR:", "gitGraph TB:", "gitGraph BT:"] {
                let mut graph = from_mermaid(&format!(
                    "{head}\n  commit id: \"release-candidate-one\" tag: \"v1.0.0-release\"\n  branch jj\n  commit\n  checkout main\n  commit\n"
                ))
                .unwrap();
                graph.look = look.clone();
                let names = vec![block("main"), short.clone()];
                let small = |words: &Option<String>| {
                    words.as_ref().map(|words| {
                        text::block(
                            words,
                            Font::new(size * 0.82, Color::BLACK),
                            text::Family::Inter,
                        )
                        .unwrap()
                    })
                };
                let words: Vec<Words> = graph
                    .commits
                    .iter()
                    .map(|commit| Words {
                        id: small(&commit.id),
                        tag: small(&commit.tag),
                    })
                    .collect();
                let plan = plan(&graph, &names, &words, 0.0);
                let plates: Vec<(f32, f32, f32, f32)> = names
                    .iter()
                    .enumerate()
                    .map(|(lane, name)| {
                        let from = plan.page(plan.start - plan.lead, plan.lanes[lane]);
                        plate_at(&plan, name, &look, from)
                    })
                    .collect();
                for (lane, &(x, y, w, h)) in plates.iter().enumerate() {
                    // Clear of the lane's first commit's mark.
                    let (cx, cy) = plan.page(plan.start, plan.lanes[lane]);
                    let apart = match graph.direction {
                        Direction::Across => cx - RADIUS - (x + w),
                        Direction::Down => cy - RADIUS - (y + h),
                        Direction::Up => y - (cy + RADIUS),
                    };
                    assert!(apart > 0.0, "{head} at {size}, lane {lane}: {apart}");
                }
                // Clear of one another, and of the first commit's words,
                // as `paint` places them.
                let meet = |(ax, ay, aw, ah): (f32, f32, f32, f32),
                            (bx, by, bw, bh): (f32, f32, f32, f32)| {
                    ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah
                };
                assert!(!meet(plates[0], plates[1]), "{head} at {size}: {plates:?}");
                let at = plan.at(0, 0);
                let (id, tag) = (
                    words[0].id.as_ref().unwrap(),
                    words[0].tag.as_ref().unwrap(),
                );
                let written = if graph.direction == Direction::Across {
                    let below = at.1 + RADIUS + 4.0;
                    let above = at.1 - RADIUS - 8.0 - tag.height - 3.0;
                    [
                        (
                            at.0 - id.width / 2.0 - 3.0,
                            below,
                            id.width + 6.0,
                            id.height + 2.0,
                        ),
                        (
                            at.0 - tag.width / 2.0 - 6.0,
                            above,
                            tag.width + 12.0,
                            tag.height + 6.0,
                        ),
                    ]
                } else {
                    let x = at.0 + RADIUS + 8.0;
                    [
                        (
                            x - 3.0,
                            at.1 - id.height / 2.0 - 1.0,
                            id.width + 6.0,
                            id.height + 2.0,
                        ),
                        (
                            x + id.width + 8.0,
                            at.1 - tag.height / 2.0 - 3.0,
                            tag.width + 12.0,
                            tag.height + 6.0,
                        ),
                    ]
                };
                for plate in &plates {
                    for words in written {
                        assert!(
                            !meet(*plate, words),
                            "{head} at {size}: {plate:?} and {words:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_statements_are_run_as_git_would() {
        let graph = from_mermaid(FLOW).unwrap();
        let names: Vec<&str> = graph.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["main", "develop", "hotfix"]);
        let told: Vec<(usize, Option<usize>, Option<From>)> = graph
            .commits
            .iter()
            .map(|c| (c.branch, c.parent, c.from))
            .collect();
        assert_eq!(
            told,
            [
                (0, None, None),
                (1, Some(0), None),
                (1, Some(1), None),
                (0, Some(0), None),
                (0, Some(3), Some(From::Merged(2))),
                (2, Some(4), None),
                (1, Some(2), Some(From::Picked(0))),
            ]
        );
        assert_eq!(graph.commits[0].id.as_deref(), Some("start"));
        assert_eq!(graph.commits[1].id, None, "an id Mermaid would make up");
        assert_eq!(graph.commits[2].tag.as_deref(), Some("v0.9"));
        assert_eq!(graph.commits[3].kind, Kind::Highlight);
        assert_eq!(graph.commits[4].tag.as_deref(), Some("v1.0"));
        assert_eq!(graph.commits[5].kind, Kind::Reverse);
        assert_eq!(graph.direction, Direction::Across);
    }

    #[test]
    fn the_graph_is_drawn_with_its_names_ids_and_tags() {
        let svg = draw(&from_mermaid(FLOW).unwrap()).unwrap();
        for words in [
            "main", "develop", "hotfix", "start", "feature", "v0.9", "v1.0",
        ] {
            assert!(svg.contains(&format!(">{words}<")), "{words}: {svg}");
        }
        // Seven commits: four dots, the merge's ring with its dot, the
        // cherry-pick's dot with its two seeds; the seventh is a square.
        assert_eq!(svg.matches("<ellipse").count(), 4 + 2 + 3, "{svg}");
        assert!(svg.contains(&Color::hex(LANES[1]).to_string()), "{svg}");
    }

    #[test]
    fn time_runs_down_or_up_with_the_words_beside_the_commits() {
        let source = |way: &str| {
            format!("gitGraph {way}:\n  commit id: \"one\"\n  commit id: \"two\" tag: \"t\"\n")
        };
        let ys = |svg: &str| -> Vec<f32> {
            svg.split("<ellipse")
                .skip(1)
                .map(|rest| {
                    let at = rest.find("cy=\"").unwrap() + 4;
                    rest[at..].split('"').next().unwrap().parse().unwrap()
                })
                .collect()
        };
        let down = from_mermaid(&source("TB")).unwrap();
        assert_eq!(down.direction, Direction::Down);
        let drawn = ys(&draw(&down).unwrap());
        assert!(drawn[0] < drawn[1], "{drawn:?}");
        let up = from_mermaid(&source("BT")).unwrap();
        assert_eq!(up.direction, Direction::Up);
        let drawn = ys(&draw(&up).unwrap());
        assert!(drawn[0] > drawn[1], "{drawn:?}");
        assert_eq!(
            from_mermaid("gitGraph LR:\n  commit\n").unwrap().direction,
            Direction::Across
        );
    }

    #[test]
    fn lanes_follow_the_order_their_branches_ask_for() {
        let source = "gitGraph\n  commit\n  branch one order: 3\n  branch two order: 1\n  branch three\n  commit\n";
        let graph = from_mermaid(source).unwrap();
        let names: Vec<&str> = graph.branches.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["main", "three", "two", "one"]);
        assert_eq!(graph.commits[1].branch, 1);
    }

    #[test]
    fn what_git_would_refuse_is_refused_by_name() {
        for (source, line, what) in [
            (
                "gitGraph\n  commit\n  checkout nowhere\n",
                2,
                "a branch that is not there",
            ),
            (
                "gitGraph\n  commit\n  branch main\n",
                2,
                "a branch named twice",
            ),
            (
                "gitGraph\n  commit\n  merge main\n",
                2,
                "a branch merged into itself",
            ),
            (
                "gitGraph\n  commit\n  branch a\n  checkout main\n  merge a\n",
                4,
                "a merge that brings nothing",
            ),
            (
                "gitGraph\n  commit id: \"a\"\n  commit id: \"a\"\n",
                2,
                "a commit id used twice",
            ),
            (
                "gitGraph\n  commit id: \"a\"\n  cherry-pick id: \"b\"\n",
                2,
                "a cherry-pick of a commit that is not there",
            ),
            (
                "gitGraph\n  commit id: \"a\"\n  cherry-pick id: \"a\"\n",
                2,
                "a cherry-pick from its own branch",
            ),
            ("gitGraph\n  commit type: LOUD\n", 1, "a commit type"),
            ("gitGraph\n  commit msg: \"hi\"\n", 1, "a commit option"),
            ("gitGraph\n  rebase main\n", 1, "a git graph statement"),
            ("gitGraph sideways:\n  commit\n", 0, "a git graph direction"),
        ] {
            assert_eq!(
                mermaid_refusal(source, &Settings::default()),
                Some((line, what)),
                "{source}"
            );
            assert!(from_mermaid(source).is_none());
        }
        assert_eq!(
            mermaid_refusal("timeline\n  2020 : x\n", &Settings::default()),
            None
        );
        assert_eq!(mermaid_refusal(FLOW, &Settings::default()), None);
    }

    #[test]
    fn a_title_comments_and_the_other_spellings_are_read() {
        let source = "---\ntitle: Releases\n---\n%% a comment\ngitGraph:\n  commit %% the first\n  branch \"release/1.0\"\n  switch main\n  accTitle: a graph\n  commit\n  merge \"release/1.0\" id: \"m\" type: REVERSE\n";
        let graph = from_mermaid(source).unwrap();
        assert_eq!(graph.title.as_ref().unwrap().value, "Releases");
        assert_eq!(graph.branches[1].name, "release/1.0");
        assert_eq!(graph.commits.len(), 3);
        assert_eq!(graph.commits[2].id.as_deref(), Some("m"));
        assert!(draw(&graph).unwrap().contains(">Releases<"));
        // A branch may start before anything is committed.
        let early = from_mermaid("gitGraph\n  branch a\n  commit\n").unwrap();
        assert_eq!(
            (early.commits[0].branch, early.commits[0].parent),
            (1, None)
        );
        assert!(
            draw(&from_mermaid("gitGraph\n").unwrap()).is_ok(),
            "an empty graph"
        );
    }
}
