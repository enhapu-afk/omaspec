# 0027: A line may run within a rank

Status: accepted, 2026-11-12. Extends [0021](0021-boxes-held-in-place.md).

## Context

The layout ranks boxes along the flow and runs every line from one rank
to another. An architecture drawn by hand does not look like that: a
gateway stands beside the user who calls it, a router beside the
orchestrator that asks it, and the line between them is short and
level. PlantUML writes this as a direction in the arrow (`a -right-> b`,
`a -left-> b`); Omaspec declined every such line.

[0021](0021-boxes-held-in-place.md) already puts a box into the rank of
another, right after or before it, and refuses to when a line joins the
two, since one rank could not hold both ends of a line.

## Decision

1. **A line with a direction across the flow runs within the rank.**
   `a -right-> b` where the flow runs down (`-down->` where it runs to
   the right) holds `b` in the rank of `a`, right after it; `-left->`
   (`-up->`), right before it. The two are ranked as one and stand side
   by side, as a hold of 0021 places them.
2. **The line is straight.** It runs between the sides that face each
   other, in the middle of where the two boxes overlap along the flow,
   its label over its middle (beside it where the flow runs to the
   right). The box that stands first keeps room after it for the label
   and the heads. The ranks know nothing of the line: it passes no rank,
   takes no track and joins no bundle.
3. **A direction along the flow is the flow.** `-down->` is a line like
   any other; `-up->` runs from its second box to its first, its heads
   and multiplicities with it, so the second box stands above.
4. **Where it cannot hold, the line is a line like any other.** Two
   boxes another line joins from rank to rank, boxes of different
   frames, or holds that go round keep their ranks, and the line is
   routed between them as before. Between boxes of different packages
   the structure reader declines a direction and names the line.
5. **A hidden line with a direction holds without a line**
   (`a -[hidden]right- b`): the hold of 0021, written in the source.
6. **Holds are placed anchor first.** A box is moved beside its
   neighbor before the boxes held beside it are, however the entries
   were written, so `b right of a` and `a right of c` stand as
   `c a b`.

## Consequences

- Only the structure diagrams read a direction so far. The state
  drawer and Mermaid's drawers pass `level: None`; a state diagram's
  `-right->` stays declined.
- Two boxes held after one (`a -right-> b`, `a -right-> c`) stand as
  `a b c`, and the line to `c` passes `b`. The writer sees it and
  chooses another side.
- A box with a self-loop keeps its loop in the room after it, where a
  line within the rank to a box held after it runs too.
