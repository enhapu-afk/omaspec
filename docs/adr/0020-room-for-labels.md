# 0020: A label takes room in the layout, as in Graphviz

Status: accepted, 2026-11-06 (proposed 2026-11-05). Extends [0012](0012-lines-like-graphviz.md) and
[0013](0013-containers-like-graphviz.md); the first step of 0.9 in the
[roadmap](../../ROADMAP.md), *a layout you can steer*.

## Context

The layered layout (`omaspec_uml::graph`) places the boxes of each rank
across the flow by their widths alone. A line's label is put afterwards, in
the gap between two ranks, beside its line (`place_labels`, `label_spot`).
Where lines run closer than their labels are wide there is no room, and the
label lies on another line or on other words. `tests/lines` counts 8 such
words over the repository's diagrams, all of that kind: five belong to a
pair of lines side by side whose labels go outside (0012, rule 4) onto a
third line 43 to 73 units off, for a label 67 to 81 wide; two lie between a
pair and a third line; one beside a container too narrow for it.

Making room where the lines meet their boxes was tried and left
(2026-11-02): squeezing every step to fit took the words from 8 to 5 but
pressed pairs to 5 units apart; taking only spare room took them to 7 for 6
more bends. The room has to come from where the boxes stand, before the
lines are laid.

How steady a layout is is now measured too (`tests/lines`, from
2026-11-05): written with each run of lines in the reverse order, the
diagrams cross 17 times instead of 7; and one box added to each moves the
others by 908 units, summed over the diagrams of how far a box moves on
average in each, the two pictures aligned on their median shift. Neither
may grow.

## Decision

A label is a node of its size in the layout, as `dot` makes it:

1. **Ranks are doubled.** A box of rank *r* takes rank *2r*, so every line
   between two boxes passes a rank between them, where it has a point of
   its own, as a line that skips ranks has already. The room between two
   ranks is halved, so a diagram without labels keeps its length.
2. **A labelled line's point is its label.** In the rank between its ends
   (the middle one, for a line that skips ranks) the point is a box as wide
   as its label and as long, plus the gap a label keeps from its line. The
   line runs along the side of that box, the label lies in it. Ordering and
   placement then treat it as any box: neighbors keep its width free, and a
   pair of lines bends around their labels instead of lying across them.
3. **Rule 4 of 0012 stays.** Of a pair side by side, the first line's label
   box lies on its outer side, and so the label.
4. **Frames double with the ranks** (0013): a container laid out with the
   lines around it spans the doubled ranks of what it holds; its band and
   its borders keep their room.
5. **What stays as it is:** self-loops, which keep their room beside their
   box; lines between containers laid out on their own (`graph::across`),
   whose labels are still put afterwards; and the lines by hand of
   [0016](0016-lines-by-hand.md), which keep their corners as shares of the
   boxes' span.

## As built

Building it (`graph::doubled`) refined the decision where the first
pictures showed why:

- **The label's rank.** A line that skips ranks has several ranks between
  ranks of boxes; its label takes the one nearest the middle, the lower of
  two, where the gap that held it was before. A rank of boxes never holds
  a label.
- **Ordering as before.** The sweeps alone, on twice the ranks, got stuck
  where they had not (a long line under a box instead of above it). So the
  ranks of boxes are ordered as before, each line's point between them
  skipped, and each rank between follows where its lines go on to, then
  where they come from. Lines cross only where they bend, as often as
  before.
- **Points onto their lines.** Once the lines meet their boxes, each point
  in a rank between moves to where its line enters the next slot, so the
  line bends once, a little past the box it leaves, or runs straight; a
  label's point, where the two boxes of its line overlap across the flow,
  into that overlap. A line without a label meets its boxes as before,
  heading for the box at its other end, and its point follows. A point on
  a frame's border follows its line too, and a point before a line's pass
  beside a frame's name heads for where the line comes down beside it.
- **The point of a short line is not held.** While the ranks are placed,
  a point of a long line stays where it still reaches the flat box it
  comes from or goes to, so the line stays straight. The one point of a
  line from one rank of boxes to the next heads for the boxes' middles
  instead, as the boxes did for each other before; held, it let boxes
  drift apart to the edge of their reach (`flow-subgraphs.mmd`).
- **Frames.** A line from one rank of boxes to the next passes between
  them inside the frames around both ends only: it crosses a frame's side
  in the gap, as before, instead of being held inside a frame that only
  reaches that rank.
- **Rule 4, by neighbors.** A label takes the other side of its line when
  its usual side has a neighbor that comes from the same slot or goes on
  to the same one, and the other side has none: of a pair into one box or
  out of one, and of a line and the one back.
- **Lines into a box met in the middle** (a circle, a diamond) keep no room
  between their points, so they run together into it, their labels
  outside.
- **Gaps.** Half the room before a rank between, half after; a gap where a
  line bends on its way into a box keeps the other half after its last
  track, for the arrow.
- **Small steps.** `straight::unjog` keeps lines off the labels of others,
  moves a label with its own line, and looks again from the start after
  each step it takes out.
- **What else changed:** `graph::ports` no longer makes room for labels
  between lines meeting one side (the rank between does; 0012, rule 2, is
  kept by the rank between), and a hidden line keeps the room of its
  label, which the signal flow puts there.
- **A line by hand** ([0016](0016-lines-by-hand.md)) with one corner below
  two boxes side by side, which now stand further apart, lost that corner:
  `hand::shape` ran down to it and straight back, and straightening
  dropped the spike. It turns there now.

Tried and left: boxes lining up through the point of a line to the box
beyond (boxes moved 920 → 955, more crossings in random graphs); no room
between points without a label (random graphs crossed 3058 → 4833); every
point pulled toward the rank after it (more bends and jogs); ports closer
to a box's corners (jogs 4 → 9).

## Results

`tests/lines` over the 54 diagrams, before and after:

| measure | before | after |
|---|---|---|
| words in the way | 8 | 0 |
| crossings | 7 | 6 |
| crossings, runs written the other way round | 17 | 15 |
| lines through boxes | 0 | 0 |
| jogs | 1 | 1 |
| lines too near | 13 | 13 |
| boxes moved by one box added | 908 | 932 |
| bends | 404 | 478 |
| length | 62,009 | 67,809 |

One measure that was not to grow did, and is taken knowingly: **boxes
moved, 908 → 932** (2.6 %), summed over the diagrams. For single diagrams
it went up and down, most where labels now take room (`state-loops.mmd`
38 → 23, `platform.mmd` 60 → 43, `history.puml` 35 → 46, where the box
added turns the order inside `On` around). The one jog left is in
`state-loops.mmd`: three lines and a label between two of them are wider
than the box they leave (`nested.puml` had the jog before).

The pictures are 1.9 % wider and 2.9 % longer over the corpus, nearly all
of it in diagrams with labels, in room the labels did not have
(`state-loops.mmd` 30 % longer, its words apart now). Of the diagrams
without labels, `flow-subgraphs.mmd` keeps its length (559 units, as
before); against rule 1, `flow-subgraph-direction.mmd`, which runs left to
right, is 6 % longer (270 → 286), its two containers further apart. Open.

42 golden SVGs changed (26 Mermaid, 16 PlantUML), each looked at next to
its old picture; 13 of them again after the two refinements above for
short lines and passes beside names (2026-11-06). `cargo bench -p omaspec-render` and the scale test
(`OMASPEC_SCALE=1`, up to 2,001 diagrams) took the same time before and
after.

## How it is checked

- `tests/lines`: words in the way go down from 8; crossings, lines through
  boxes, jogs, the crossings written the other way round and how far boxes
  move may not grow. Bends and length may change.
- The golden SVGs of diagrams with labels change. Each one is looked at
  next to its old picture before it is taken.
- The timings of `cargo bench -p omaspec-render` and the scale test
  (`OMASPEC_SCALE=1`) are measured before and after; a typical diagram
  still draws in milliseconds.

## Consequences

- Every edge gets at least one point, so the layout handles more slots:
  one per line per rank passed.
- The gap before a label shrinks to what the label rank leaves; a line
  bends at most once more on its way past a label.
- `tests/lines.txt` was written anew; the per-diagram counts it holds are
  the ones to keep from here.
- Open: the labels of lines between containers laid out on their own,
  which `graph::across` routes after placement; the jog in
  `state-loops.mmd`; the length of `flow-subgraph-direction.mmd`.


## Addendum (2026-11-09): off the curves

Drawn, a line's corners turn in curves of up to 48 units (`curve::RADIUS`),
and a label at the top of its rank sat where the curve of the corner above
still reached: its first letter was cut. `doubled::label_beside` now moves
the label along the straight run through its rank until the curve on its
side stays the label's gap off it, and on a run too short for that sets it
in the run's middle, off the line by what the curve reaches there.
`tests/lines` measures words against the lines as drawn since.
