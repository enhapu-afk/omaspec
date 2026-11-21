//! The git graph's reader and drawer, tried.

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
