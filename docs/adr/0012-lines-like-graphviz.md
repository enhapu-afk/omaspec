# 0012: Lines follow the rules Graphviz lays them out by

Status: accepted, 2026-10-28. Changes where the routers of state, structure
and Mermaid graph diagrams lay lines; how corners turn stays as in
[0011](0011-curved-lines.md).

## Context

The gallery's graph examples (coffee machine, elevator, media player,
crossing, bookshop, library, smart home, ticket machine) were drawn by
Omagram and by Java PlantUML, which lays them out with Graphviz `dot`, and
compared side by side. Java's lines read better: they run short and direct,
where Omagram sent every way back around the whole diagram and joined
several lines at one point of a box. Where the two differed, Graphviz
followed a handful of rules Omagram did not.

## Decision

1. **A back edge is a forward edge drawn backward.** It runs through the
   ranks between its two boxes, with a dummy point in each rank it skips,
   ordered and straightened with the other lines, and is drawn from its
   source back up to its target. A transition and the one back (power on
   and off, play and pause) lie side by side between the two boxes, not in
   a gutter around the diagram.
2. **Lines share a side.** Lines that meet one side of a box spread along
   it in the order of their other ends, so none crosses another at the box,
   16 units apart where the side allows and further where a label lies
   between two of them. A circle or a diamond is met in the middle; only
   lines to the same box lie side by side there, and lines to other boxes
   keep their order with them.
3. **A line heads for its other end.** Where the two boxes overlap across
   the flow, the line runs straight through the middle of the overlap;
   otherwise it meets each side at the point nearest the other box. A few
   passes then let the two ends of each line follow each other, short sides
   first, so a pair squeezed onto a small box still runs straight to a big
   one.
4. **Labels of lines side by side go outside.** Of several lines into one
   side, the first carries its label on the other side of it (left when the
   flow runs down, below when it runs across).
5. **A container is open from above.** Only its name keeps lines off: a
   package's tab, the middle of the band of any other container. A line
   from outside comes in through the side facing its other end.
6. **The direction belongs to the diagram.** `left to right direction` in a
   structure diagram holds inside packages and rectangles too, wherever it
   is written, as Graphviz's `rankdir` does.

Not taken from Graphviz: free splines (the routers still lay straight runs
whose corners curve, [0011](0011-curved-lines.md)), and one ranking across
all containers (each container is still laid out on its own; lines between
containers are routed afterwards, `graph::across`; since taken up in
[0013](0013-containers-like-graphviz.md)).

## Consequences

- The gutters for back edges (`graph::back`) are gone. A hidden back edge,
  one that only ranks two containers, still gets no chain, so it does not
  pull on the ranks it would pass.
- `graph::Node` says whether lines may meet the box anywhere along a side
  (`flat`), as `graph::across::Placement` already did.
- Golden SVGs of state and structure diagrams changed; the layout tests
  check the new rules.
- Open: a line between containers can still detour where one ranking for
  the whole diagram would have placed its boxes nearer (the smart home), and
  its label does not push aside the labels placed before it. Taken up in
  [0013](0013-containers-like-graphviz.md).
