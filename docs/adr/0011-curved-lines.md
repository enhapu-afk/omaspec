# 0011: Lines between states and boxes curve

Status: accepted, 2026-10-27. Changes the "straight or right-angled lines" of
[0006](0006-goals-and-scope.md) for state, structure and Mermaid graph diagrams.

## Decision

The lines of state diagrams (PlantUML and Mermaid), of structure diagrams
(class, component, use case, deployment, object) and of Mermaid flowcharts
turn in curves. The routers still lay each line out as straight runs that
keep clear of boxes and labels; only the drawing changes. Every corner
turns in a curve that starts at most 48 units before it, and no further
than half of either run (the first and the last run whole). A short jog
becomes an S, a detour around the diagram an arc. The ends keep their
direction, so arrowheads sit on the line. Code: `omaspec_uml::curve`.

Right angles stay where they belong to the type, and where the file asks:

- Activity, sequence, timing and signal-flow diagrams keep straight and
  right-angled lines.
- `skinparam linetype ortho` (or `polyline`) keeps the corners of a
  PlantUML state or structure diagram; another value declines.
- A Mermaid flowchart with `"curve": "linear"` or a step curve in
  `%%{init: {"flowchart": …}}%%` keeps its corners. A Mermaid state
  diagram always curves.

## Why

The owner found right angles stiff, most of all in state machines and use
cases, where several lines detour around the diagram. PlantUML (through
Graphviz) and Mermaid curve these lines by default, so curves are also
what a reader of either expects.

Rounding the router's corners keeps what the router guarantees: a curve
never leaves the corridor its runs lay in, so it does not cross a box the
straight line did not. Free splines through the route's points would
smooth more, but cut corners by a third of a run and could cross boxes.

## Consequences

- A play's light runs along the curve: the map of a state diagram holds
  the drawn curve as points, its ends unchanged.
- The routing tests keep checking the router with straight runs; the
  golden SVGs of state and structure diagrams hold the curves.
