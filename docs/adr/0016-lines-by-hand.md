# 0016: Lines by hand live in the source

Status: accepted, 2026-10-31; revised the same day, after a check in a real
window: the line keeps the shape the writer gives it (see
[Earlier](#earlier)). Takes up "Lines by hand" from the roadmap's
[Next](../../ROADMAP.md#next).

## Context

The routers lay every line ([0011](0011-curved-lines.md),
[0012](0012-lines-like-graphviz.md), [0013](0013-containers-like-graphviz.md)),
and sometimes a writer knows a better way: around a box rather than past it,
off another line, clear of a label. Graphviz takes no hints of that kind,
and neither PlantUML nor Mermaid has a way to write one down.

## Decision

1. **The shape is visible source.** A block of comments at the end of the
   diagram holds it, as a skin pack holds a look
   ([`skin.rs`](../../crates/omaspec-core/src/skin.rs)): it travels with the
   file, its copies, git and the images that carry their source. Java
   PlantUML, Mermaid and mmdr read only comments in it, so the file stays
   what it was everywhere else.

   ```plantuml
   Idle --> Running : start
   Running --> Idle : stop

   ' lines by hand (Omaspec)
   ' Idle -> Running at 0.5 -0.4
   ' Running -> Idle at 1.2 0.3, 1.2 0.8
   @enduml
   ```

   In PlantUML the block stands before the `@end…` line; in Mermaid it is
   `%%` comments at the end, inside a code fence if there is one. A blank
   line goes before it.
2. **A line is named by its ends.** `From -> To` with the names the lines
   use (an alias where there is one), and `#2`, `#3` … for the second and
   later lines between the same two, in the order of the source. A name with
   a space or a quote is written in double quotes. A source line number
   would change with every line written above it.
3. **An entry holds the line's corners.** Each pair after `at` is a corner,
   as a share of the span the line's two boxes take together, across and
   down on their own: `0.5 -0.4` is half way across that span and 40 % of
   its height above it. When the diagram changes, the shape stretches with
   its boxes, and corners in line stay in line, so runs stay straight. A
   line takes at most 32 corners; shares are written to a thousandth.
4. **The line is drawn as written; no router lays it.** The layout runs as
   before: ranks, order and boxes do not move for a line by hand. Then each
   such line is drawn through its corners in straight runs across and down
   (`hand::shape`): it leaves its first box straight toward the first
   corner where the box's side reaches, or from the nearest side with a
   corner beside the box where it does not, and enters its last box the
   same way. Two corners out of line get a corner between them. Its label
   is placed along it, clear of the boxes and the labels already there.
   Curves ([0011](0011-curved-lines.md)) and `linetype ortho` hold as
   before, but a corner of a line by hand is rounded by at most 8 units, so
   the line turns where the writer put the corner.
5. **What does not fit is said.** A corner inside one of the line's own
   boxes, or more than 1000 units off the diagram, is left out; a line by
   hand that runs through another box is drawn so, and said. The drawing
   names each of these, and an entry whose line is gone, by the entry's line
   (the map's `misses`); the editor lists them among the findings. Lint
   checks only how an entry is written: it does not lay the diagram out.
6. **The writer drags with Alt.** In the diagram pane of the editor and in
   the editor's diagram window, Alt shows the line under the pointer in the
   accent color with its corners as handles. Alt and a drag on a straight
   run moves the run sideways, and the corners at its ends with it; the
   pointer shows which way. Alt and a drag on a handle moves that corner,
   and the runs on either side of it with it. A run that meets a box at both
   ends gets a corner of its own when dragged, so the line slides along the
   sides of its boxes while they reach and steps around them where they do
   not. While the drag goes on, the pane draws the line as it will be drawn
   (`Wire::shaped`, the same `hand::shape` the drawers use), so what the
   writer lets go is what stays. On release only the corners the shape
   needs are written (`Wire::simplest`). Alt and a double click gives the
   line back to the layout. Escape stops a drag. Each drag is one undo
   step. A drag without Alt pans, as before; Hyprland moves windows with
   Super, so Alt is free. Commands reset the line at the caret and every
   line.
7. **The preview window only shows them.** `omaspec preview FILE` follows a
   file that another editor writes, so it draws lines by hand but does not
   write the file.
8. **Graph diagrams only.** State diagrams (PlantUML and Mermaid),
   structure diagrams (class, component, use case, deployment, object,
   Mermaid class) and Mermaid flowcharts: the types whose lines come from
   `graph`. Sequence, activity and timing diagrams keep the lines their type
   gives them. When Java PlantUML or mmdr draws instead, the block is left
   out and the status line says so.

## Why

Visible source over a side file: a side file is lost on a copy, a rename or
an export, and a reviewer does not see it in a diff. Corners relative to the
two boxes over absolute ones: absolute coordinates break with any edit that
moves a box, which is most edits; a share of the span the two boxes take
follows them both, and a share for each way keeps a straight run straight.
The shape as drawn over points a router passes: see [Earlier](#earlier).
Moving runs and corners over placing points: it is how drawing tools let a
writer shape a line at right angles (draw.io, yEd, Visio), and every drag
changes only what the pointer holds. Alt over a plain drag: a plain drag on
the paper pans, and a line is easy to hit by accident in a dense diagram.

## Earlier

The first version kept bends instead of corners: points
relative to the way between the middles of the two boxes, which the router
(`graph::route::route_via`, a search over the bends passed) laid the line
through, around every box. Checked in a real window, the result felt like
chance: the router chose on its own where the line left and entered its
boxes and where it turned between the bends, and a bend moved by 5 units
could make it choose otherwise. Moving one bend sideways in steps of 5
units on the gallery's coffee machine flipped the line's ends six times
over 240 units, some back and forth. Making the line turn at its bends and
showing the routed line while dragging helped, but did not end it. The
router still serves the lines the layout lays, through `route_via` with no
bends.

## Consequences

- `omaspec_uml::hand` reads and writes the block and shapes a line from
  its corners; `omaspec_core::hand` turns a drag into an edit of the
  source; lint checks how entries are written.
- `graph::by_hand` draws the lines by hand for all three drawers: state
  diagrams, structure diagrams (and Mermaid class diagrams), and Mermaid's
  state diagrams and flowcharts. `across::link` takes a shape and only
  labels it.
- The diagram map names every line of the graph types by its key, with its
  drawn points, its runs as laid and its two boxes, so the pane can find
  the line under the pointer and shape it as the drawers will. A play
  follows a line by hand, as it follows the drawn curve.
- `convert` carries the block over, since the keys are names; `diff` and
  the behavior model ignore it; `fmt` keeps it at the end.
- Not now: moving a label by hand, snapping a run to another line or to the
  middle of a box, signal-flow diagrams, and boxes by hand.
