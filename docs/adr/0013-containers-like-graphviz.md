# 0013: Containers are laid out with what is around them

Status: accepted, 2026-10-28. Changes how state, structure and Mermaid graph
diagrams lay out a container that a line runs into from outside; follows
[0012](0012-lines-like-graphviz.md).

## Context

After [0012](0012-lines-like-graphviz.md), Java PlantUML still drew the
gallery's diagrams with containers better: the smart home, the library,
the ticket machine. Omagram laid each container out on its own, placed it
as one box, and routed a line between two containers afterwards, around
everything (`graph::across`). The boxes could not move for such a line, so
it came in from a side, through the frame, or around the diagram.
Graphviz lays a cluster out with the whole graph: its nodes take the
graph's ranks, and a line between clusters is an edge like any other.

## Decision

A container a line runs into from outside is opened: it becomes a frame of
the layout around it (`graph::framed`), as Graphviz does with a cluster.

1. **A frame keeps its own ranks and moves down as a whole.** Each frame
   is ranked on its own, inside out; one level up it moves down as far as
   the lines between it and what is around it ask, where they leave and
   enter it. Two frames can share ranks and sit side by side (the library's
   packages step down diagonally, as in Java).
2. **A frame holds together.** Every rank keeps a frame's slots side by
   side, and sibling frames in the same order on every rank. Across the
   flow a frame keeps its padding and stays clear of every other frame on
   all the ranks they share (separation constraints, solved rank by rank
   as VPSC's "satisfy" does, the other ranks held). Along the flow the gap
   before a rank grows by the room above the frames that begin there, and
   the gap after by the room below those that end.
3. **A line runs through the ranks.** It passes each rank in the frame
   around its nearer end that reaches that rank, so it leaves a frame
   through the side the flow leaves by. Ports, tracks and labels follow
   [0012](0012-lines-like-graphviz.md).
4. **Lines pass beside a frame's name.** A line coming into a frame from
   above that would cross its name comes down beside it, on the side
   nearer where it heads, fanned out so none crosses another, and turns
   back under the name; the frame keeps room for that, and grows across
   once where the name leaves no room on either side.
5. **A line to the container itself ends on its border**, at a point of
   its own in the frame's first rank (or last, for one leaving it), so it
   keeps apart from the lines that pass into the frame there.

A container stays a box laid out on its own when no line runs into it, or
when it has concurrent regions, a direction of its own, or a line from
itself to itself. A line into such a box is still ranked where both ends
meet and routed afterwards (`graph::across`).

Not taken from Graphviz: network simplex for positions (rank-by-rank
constraint solving keeps the existing straightening), and free splines
([0011](0011-curved-lines.md)).

## Consequences

- The drivers (`structure::draw::open`, `state::draw::open`, the Mermaid
  drawer's `open`) gather the boxes of the containers they open into one
  layout and build the nested scenes from it; a crossing is drawn there or
  left in its place for the router.
- A diagram without such lines is laid out exactly as before.
- Golden SVGs of diagrams with lines between containers changed.
- Open: a line that passes a rank beside a frame can still cross it where
  the order of a rank puts it on the other side; small steps remain where a
  box beside a long line needs the room.
