# 0022: Lines into one box share a trunk

Status: accepted, 2026-11-06 (proposed the same day; amended the same
day: lines bundle only when they arrive alike). Extends
[0012](0012-lines-like-graphviz.md) and
[0020](0020-room-for-labels.md); the third step of 0.9 in the
[roadmap](../../ROADMAP.md), *a layout you can steer*.

## Context

Where many lines end at one box, each keeps its own point on that side
([0012](0012-lines-like-graphviz.md)): the ports stand 16 units apart, and
a line back sits beside the line forward. The side of a busy box becomes a
comb. Graphviz draws those lines as one for the last part of the way.
`concentrate` runs after the ranks are ordered and before the boxes are
placed: adjacent virtual nodes with one line in, one line out and no label
merge when they share a tail or a head. Its default is off.

The default here is on. Merging those points the way `dot` does would move
the boxes, and [0020](0020-room-for-labels.md) showed that a move of the
boxes moves the pictures, and the signal flow with them, since it ranks
its boxes with this layout. What reads as one arrival is the last few
pixels beside the box.

## Decision

1. **Arrivals share the last stem.** Lines that arrive on one flat side of
   one box, from different boxes, and carry no words, meet on one trunk:
   the last 12 units beside the box. Each keeps the run the layout gave it
   through the ranks and turns onto the trunk only as it arrives. The line
   in the middle of that row already lies on the trunk and stays as it
   was. The trunk stands at that line's point in the rank beside the box,
   at least 10 units in from a corner of the side.
2. **Some lines stay on their own arrival.** A line with words on it: the
   words sit on the run beside the box (0020), and several of them on one
   trunk would pile up, which `tests/lines` counts. A line with a
   multiplicity at either end: the mark sits on the stem, and two of them
   on one trunk pile up the same way. A line that arrives unlike the
   others: another head at the box (a triangle beside an arrow, a circle
   beside a cross), another color, width or dash. On one trunk the heads
   would cover each other, and a dashed line would hide under a solid one,
   so lines bundle only with lines that arrive alike. A line the writer
   kept apart. Two
   lines between the same two boxes, each with its own arrow and its own
   label. A line that leaves a box, so two boxes that overlap across the
   flow are still joined by a straight line (0012). A self-loop, a line
   into a circle or a diamond, and a line that ends on a frame.
3. **A trunk that would cross, or jog, is left undrawn.** Before a trunk
   is kept, the lines of that one layout are checked against it. A corner
   that only touches, and a run two lines already share, do not count as a
   crossing. A line that would have to move less than 8 units stays where
   it is and splits the row, so the new run is long enough to be a real
   one. A line routed afterwards, around containers laid out on their own
   ([0013](0013-containers-like-graphviz.md)), is outside that check.
4. **The writer turns it off in the block of
   [0016](0016-lines-by-hand.md).** `apart` keeps every line of the diagram
   on its own arrival. `apart at Name` does that for the lines that meet
   one box. The first of each wins; lint warns on a repeat. A name that is
   no box is a finding, as for a hold, and the lines stay bundled; `lint`
   and `check` say it too. A box
   whose name is the word `apart` is written in quotes, and
   `apart after Running` is still a hold. `rename` carries the name. The
   palette's command that resets every line and box removes the whole
   block, this entry with it. There is no drag: the layout bundles, and
   the entry is how a writer says otherwise.
5. **A line by hand is drawn as written.** It counts while the trunk is
   chosen, and then its own corners replace the route, as 0016 already
   does.
6. **Arrowheads on one trunk stack.** The later line covers the stem.
   They are one kind of head in one stroke (point 2), so the stack reads
   as one.

## Why

The last stem, over a shared run through the ranks: the words of 0020 sit
on that run, and one shared bend there pulls every line off the track it
had and across the lines beside it. While the shared part still ran back
to the rank, `tests/lines` gained a crossing in the delivery drone and
piled the two library diagrams' multiplicities on one x. The short stem,
and leaving a labelled line on its own arrival, keeps both as they were.

The points in the rank stay where they are, over merging them as `dot`
does under `concentrate`. Merging them moves the boxes. The signal flow
would move with them. The difference from Graphviz is this one, and it is
intentional.

## As built

- **Where it runs.** `graph::bundle::share`, after the small steps are
  taken out (`straight::unjog`) and before a label is placed, so the label
  follows the point the line now has. Arrivals group by their box, their
  side and their key (`Edge::arrival`, made by `graph::arrival` from the
  head at the box and the stroke; each drawer gives it through
  `Nested::arrival`). `graph::open` marks a line apart
  when either end is a box the writer kept apart. State diagrams and
  structure diagrams, and Mermaid flowcharts, states and class diagrams,
  read the entries (`hand::apart`, `hand::parted`) and say a miss with the
  other findings, which `lint`, `check` and the MCP server report as the
  editor does (`omaspec_render::by_hand_misses`).
- **The fixture.** `tests/state/bundled.puml`: *Running*, *Paused* and
  *Stopped* arrive at *Done* on one stem, and the labelled lines out of
  *Idle* stay spread.
- **What the pictures did.** Over the diagrams `tests/lines` measures,
  crossings stay 8, lines through boxes 0, jogs 1, words in the way 0, and
  crossings written the other way round stay 21. Corners that pass near
  another line go 15 → 17, one each in `flow-directions.mmd` and
  `flow-rl.mmd`. Bends go 504 → 530 and length 69,788 → 70,715: six
  diagrams gain the short turn onto a trunk (`pipeline.mmd` gains two),
  and the new fixture has 12 bends of its own. How far the boxes move
  goes 966.0 → 986.4, and the 20.4 is that fixture. Every diagram that was
  already there moves as before.

## How it is checked

- `graph::bundle`: three lines into one box arrive as one; a labelled
  line, a line with a mark at an end, a line kept apart, a line that
  arrives unlike the others, and two lines between the same boxes keep
  their own arrivals; lines that leave a box stay spread.
- `graph::nest`: of the lines into the rules of the smart home, a pair
  shares a trunk, and a point a short step away stays on its own.
- The structure drawer: the labelled lines into the hub each keep their
  arrival, clear of its name, and two multiplicities into one box stay
  apart, their marks clear of each other. An arrow, a triangle and a
  dependency into one box keep three arrivals; three arrows share one.
- The Mermaid flowchart drawer: `--o` and `--x`, and `-.->` and `==>`,
  keep their own arrivals beside `-->`.
- `tests/lines`, and the golden SVGs. Six changed, where a line turns
  onto a trunk (`flow-directions.mmd`, `flow-held.mmd`, `flow-rl.mmd`,
  `state-pseudo.mmd`, `pipeline.mmd`, `states.puml`), and `bundled.puml`
  is new. Each was looked at. `class.puml` had changed too, a solid and a
  dashed line into *Book* on one trunk; since they arrive unlike, it is
  as it was before this ADR.
- Lint warns on a repeated `apart` and a repeated `apart at`, and on an
  `apart at` or a hold that names no box (`tests/cli.rs`). `rename`
  carries the name in `apart at`.

## Consequences

- `graph::layout` may move the last stem of an arrival after the line has
  been routed.
- `omaspec_uml::hand` reads and writes `apart`. The drawers pass it
  through, as they pass holds.
- Left for later: a shared run through the ranks between the boxes,
  bundling the lines that leave a box, bundling a line that carries words,
  a drag that writes `apart`, and merging the points in the rank, which
  would move the boxes.
