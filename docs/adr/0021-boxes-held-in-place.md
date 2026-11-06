# 0021: A box held in place stands beside a neighbor

Status: proposed, 2026-11-06. Extends [0016](0016-lines-by-hand.md); the
second step of 0.9 in the [roadmap](../../ROADMAP.md), *a layout you can
steer*.

## Context

Since [0016](0016-lines-by-hand.md) a writer can shape a line by hand; a
box still stands where the layout puts it. The layout chooses well most of
the time, but not always as the writer would: a state that belongs on the
left of its sibling, a class one rank further down beside the one it
explains, a pair the sweeps put the other way round. Moving the box by
writing the source in another order is a guess the layout may not follow.

Coordinates do not hold. A theme or a font changes the size of every box,
an edit above a box moves it, and [0020](0020-room-for-labels.md) showed
that one label more moves boxes all over a diagram (`tests/lines`, how far
boxes move). What a writer means by a drag is rather *this box goes there,
beside that one*: a rank and a place in it. Graphviz says the same with
`rank=same` and the order of a rank; PlantUML and Mermaid have no way to
write it.

## Decision

1. **The block of 0016 holds boxes too.** An entry `Box after Neighbor`
   holds `Box` in the rank of `Neighbor`, right after it across the flow
   (right of it when the flow runs down, below it when it runs across);
   `Box before Neighbor`, right before it. Names are written as for lines
   (an alias where there is one; double quotes around a name with a space
   or a quote).

   ```plantuml
   ' lines by hand (Omaspec)
   ' Idle -> Running at 0.5 -0.4
   ' Paused after Running
   ' Error before Idle
   @enduml
   ```

2. **Rank and order, nothing else.** Ranking puts a held box in its
   neighbor's rank: the two are ranked as one, as `dot` ranks
   `rank=same`. Once the ranks are ordered, the box moves right after (or
   before) its neighbor in that rank. Placement then works as before, so
   the box keeps the room it takes and the gaps around it, in any theme.
   Several boxes held after one neighbor follow it in the order of the
   block; a chain (`A after B`, `C after A`) holds as written.
3. **Lines follow.** The lines of a held box are laid as any other, from
   where it stands. A line by hand on it keeps its corners as shares of the
   span of its two boxes (0016, rule 3), so it moves with the box.
4. **What does not hold is said.** An entry does not hold, and the box
   stands where the layout puts it, when a name is not in the diagram, a
   box is held beside itself, boxes it would put in one rank are joined by
   a line (one rank cannot hold both ends of a line), they lie in different
   containers (a
   box keeps to its container; moving it into another changes what the
   diagram says, which is the source's job), or the entries go round
   (`A after B`, `B after A`). The drawing names each of these by the
   entry's line (the map's `misses`), as for lines by hand; lint checks how
   an entry is written.
5. **The writer drags with Alt, as for lines.** Alt and a press inside a
   box (not on a line) holds the box: its outline follows the pointer, and
   a bar shows where it would go, between two boxes of the rank nearest the
   pointer. On release the entry is written, `after` the box before the
   bar, or `before` the first box of the rank when the bar stands first; a
   drop where the box stands already writes nothing, and a drop where it
   cannot go (beside a box it is joined to, into another container) shows
   no bar and writes nothing. Alt and a double click on a box gives it back
   to the layout. Escape stops a drag; each drag is one undo step. The
   commands that reset the lines by hand reset the boxes too.
6. **Names change with the box.** `omaspec rename` and the editor's rename
   carry the names in the block's entries, of boxes and of lines, so a
   renamed box keeps its place and a renamed line its shape.
7. **The same diagrams as 0016.** State diagrams (PlantUML and Mermaid),
   structure diagrams and Mermaid flowcharts and class diagrams: the types
   laid out by `graph`. A container laid out on its own is a box of the
   level around it and can be held there; what it holds is held inside it.
   When Java PlantUML or mmdr draws instead, the entries are left out, as
   lines by hand are.

## Why

Beside a neighbor over coordinates: sizes change with a theme, a font or
one label more, and a box at fixed coordinates would overlap its
neighbors or leave holes. Beside a neighbor over a rank number: the number
of a rank changes with every box added above it; a neighbor moves with the
diagram. Rank and order only, not a shift across: placement keeps every
box clear of the others and its lines straight where it can, which a shift
by hand would undo. A held box is ranked with its neighbor, not below a box
of the rank before: the drop shows a place among boxes, and that is what is
written.

## How it is checked

- `graph`: held boxes take their neighbor's rank and stand next to it, in
  frames too; an entry that does not hold leaves the layout as it was.
- `tests/lines` and the golden SVGs do not change: no diagram there holds a
  box. A new fixture in each drawer holds one, as a golden.
- The editor: a drag on a box writes its entry, a double click takes it
  out, Escape writes nothing; headless snapshots of the bar during a drag.
- `rename`: a held box renamed keeps its entry; so does a line by hand.

## Consequences

- `omaspec_uml::hand` reads and writes a second form of entry;
  `omaspec_core::hand` turns a drop into an edit of the block.
- `graph::framed` takes the holds: in `ranks` and `nest::ranks` a held box
  and its neighbor are ranked as one; `order_ranks` moves held boxes after
  the sweeps. The drawers turn names into the layout's boxes, as they do
  for lines by hand.
- The diagram map names every box a writer can hold, with its rank and its
  container, so the pane can show the bar; the map's `misses` take the
  entries of boxes that did not hold.
- Not now: a box in a rank of its own (below the last, or between two
  ranks), a shift across within its place, holding a container laid out
  with what is around it (a frame of [0013](0013-containers-like-graphviz.md))
  as a whole, and the signal flow.
