# 0010: Signal flow is Omagram's own diagram

Status: accepted, 2026-10-24. An exception to the "no new language" rule
in [0007](0007-behavior-specifications.md), for this kind only.

## Decision

`@startflow` is a diagram kind of Omagram's own. PlantUML has no
signal-flow form, so there is nothing to stay syntax-compatible with and
the Java PlantUML is not the fallback. A line the native engine declines
is reported as that line, and is not sent to Java.

This does not replace PlantUML or Mermaid for the types those already
draw. Sequence, activity, state and flowchart stay what they are.

A signal flow is software as blocks that receive and send named signals.
The first slice draws that picture: blocks, named ports, wires from an
output to an input, left to right. The graph engine only places the
blocks. The wires are drawn to the ports, because a graph edge ends at
the center of a box. The diagram does not run, and it is not a behavior
model.

## Why

The pictures the behavior types draw are conversations, states and steps.
A signal flow is the other view of the same software: which block sends
which named signal to which block. Component diagrams were not extended
with ports. Their lines end at the box, and the structure parser rejects
`port`.

## Rules

- Names are case sensitive. A port is `block.port` or a boundary `in` or
  `out`. An input takes one wire. An output may fan out.
- A block may contain blocks and wires. That block is drawn as a frame,
  and its ports are the signals that cross the frame. A bare name inside
  the frame is one of those ports. An inner block's name stays inside
  the frame.
- The only direction is left to right.
- The look is the engine default. The Omarchy theme is laid over it
  through the skinparams the packs already write (`backgroundColor`,
  `defaultFontColor`, `ArrowColor`, `RectangleBackgroundColor`,
  `RectangleBorderColor`). No new house style.
- Not in this slice: buses, a Simulink import, checks, and running the
  diagram.
