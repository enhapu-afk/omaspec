# 0020: A label takes room in the layout, as in Graphviz

Status: proposed, 2026-11-05. Extends [0012](0012-lines-like-graphviz.md) and
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
- Open: the labels of lines between containers laid out on their own,
  which `graph::across` routes after placement.
