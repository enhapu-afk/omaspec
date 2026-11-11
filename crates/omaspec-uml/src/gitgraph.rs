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
            font_name: "sans-serif".into(),
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
    read::read(source).ok()
}

/// The first construct [`from_mermaid`] refuses by name, and its line.
/// A source that is not a git graph, and one it accepts, are `None`.
pub fn mermaid_refusal(source: &str) -> Option<(usize, &'static str)> {
    match read::read(source) {
        Err(read::Stop::At(reason)) => Some((reason.line, reason.what)),
        _ => None,
    }
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
                    .map(Some)
                    .map_err(|reason| Declined::at(commit.line, reason)),
                None => Ok(None),
            };
            Ok(Words {
                id: block(&commit.id)?,
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
    Ok(canvas.finish(10.0, look.background))
}

/// Lays the lanes and the turns out so that no words meet.
fn plan(diagram: &Diagram, names: &[Block], words: &[Words], top: f32) -> Plan {
    let across = diagram.direction == Direction::Across;
    let turns = diagram.commits.len().max(1);
    if across {
        // The names stand before the lanes; an id stands under its
        // commit, so the widest of them sets how far the turns are apart.
        let widest_name = names.iter().map(|block| block.width).fold(0.0, f32::max);
        let widest = words
            .iter()
            .flat_map(|w| [&w.id, &w.tag])
            .flatten()
            .map(|block| block.width)
            .fold(0.0, f32::max);
        let step = STEP.max(widest + 14.0);
        let lanes = (0..diagram.branches.len())
            .map(|lane| lane as f32 * LANE + LANE / 2.0)
            .collect();
        let start = widest_name + 20.0 + LEAD;
        return Plan {
            direction: diagram.direction,
            lanes,
            start,
            step,
            end: start + (turns - 1) as f32 * step,
            top,
        };
    }
    // The names stand over the lanes; the words stand beside their commit,
    // so each lane is as wide as its name and its longest words ask.
    let mut lanes = Vec::with_capacity(names.len());
    let mut edge = 0.0;
    for (lane, name) in names.iter().enumerate() {
        let beside = diagram
            .commits
            .iter()
            .zip(words)
            .filter(|(commit, _)| commit.branch == lane)
            .map(|(_, words)| words.reach())
            .fold(0.0, f32::max);
        let half = (name.width / 2.0 + 10.0).max(RADIUS + 6.0);
        lanes.push(edge + half);
        edge += (half + (half.max(RADIUS + 6.0 + beside)) + 12.0).max(LANE);
    }
    let start = names.iter().map(|block| block.height).fold(0.0, f32::max) + 12.0 + LEAD;
    Plan {
        direction: diagram.direction,
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

    // Each lane: a faint line under its commits, and its name before it.
    for (lane, name) in names.iter().enumerate() {
        let color = lane_color(look, lane);
        let l = plan.lanes[lane];
        let from = plan.page(plan.start - LEAD, l);
        let to = plan.page(plan.end + plan.step / 2.0, l);
        let faint = Stroke::new(color.mix(paper, 0.55), 1.5).dashed(3.0, 4.0);
        canvas.line(Layer::Background, from, to, faint);
        let (w, h) = (name.width + 16.0, name.height + 8.0);
        let (x, y) = match plan.direction {
            Direction::Across => (from.0 - 8.0 - w, from.1 - h / 2.0),
            Direction::Down => (from.0 - w / 2.0, from.1 - 8.0 - h),
            Direction::Up => (from.0 - w / 2.0, from.1 + 8.0),
        };
        canvas.card(Layer::Bars, (x, y, w, h), 4.0, color);
        canvas.text(Layer::Front, name, (x + w / 2.0, y + 4.0), Align::Center);
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
            if let Some(id) = &written.id {
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
            assert_eq!(mermaid_refusal(source), Some((line, what)), "{source}");
            assert!(from_mermaid(source).is_none());
        }
        assert_eq!(mermaid_refusal("timeline\n  2020 : x\n"), None);
        assert_eq!(mermaid_refusal(FLOW), None);
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
